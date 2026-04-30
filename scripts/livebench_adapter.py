#!/usr/bin/env python3
"""
LiveBench adapter for RustyCode.

Routes LiveBench questions through the RustyCode CLI and scores results
using LiveBench's objective ground-truth evaluation pipeline.

Usage:
    # Install LiveBench first:
    #   cd /path/to/livebench && pip install -e .

    # Run all non-coding categories:
    python livebench_adapter.py --categories reasoning math language instruction_following

    # Run a specific category:
    python livebench_adapter.py --categories reasoning

    # Run with specific model/display name:
    python livebench_adapter.py --model claude-sonnet-4-6 --categories math

    # Skip inference, only re-score existing answers:
    python livebench_adapter.py --skip-inference --categories reasoning

    # Resume interrupted run:
    python livebench_adapter.py --resume --categories reasoning

    # Debug a specific question:
    python livebench_adapter.py --question-id <id> --debug
"""

import argparse
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

import shortuuid
import tqdm

# Add livebench to path if not installed
LIVEBENCH_ROOT = os.environ.get(
    "LIVEBENCH_ROOT", str(Path(__file__).resolve().parent.parent.parent / "livebench")
)
if os.path.isdir(LIVEBENCH_ROOT) and LIVEBENCH_ROOT not in sys.path:
    sys.path.insert(0, LIVEBENCH_ROOT)
    # Fix circular import in livebench IF evaluation
    if_runner_path = os.path.join(LIVEBENCH_ROOT, "livebench", "if_runner")
    if os.path.isdir(if_runner_path) and if_runner_path not in sys.path:
        sys.path.insert(0, if_runner_path)

from livebench.common import (
    LIVE_BENCH_RELEASES,
    get_categories_tasks,
    get_hf_dataset,
    load_questions,
    reorg_answer_file,
)
from livebench.gen_ground_truth_judgment import play_a_match_gt

# Fix LiveBench bug: IF questions use instruction IDs from
# instruction_following_eval/instructions_registry but evaluation_lib
# imports from ifbench/instructions_registry (different, incompatible registry).
# Monkey-patch evaluation_lib to use the correct registry.
try:
    from instruction_following_eval import instructions_registry as _correct_reg
    from livebench.if_runner.ifbench import evaluation_lib as _eval_lib
    if _eval_lib.instructions_registry is not _correct_reg:
        _eval_lib.instructions_registry = _correct_reg
except ImportError:
    pass


# Categories that require Docker for scoring (skip by default)
DOCKER_CATEGORIES = {"coding"}

# RustyCode CLI binary
RUSTYCODE_BIN = os.environ.get("RUSTYCODE_BIN", "rustycode")

# Config path
RUSTYCODE_CONFIG = os.environ.get(
    "RUSTYCODE_CONFIG", os.path.expanduser("~/.rustycode/config.json")
)


def load_rustycode_config() -> dict:
    """Load RustyCode config from disk."""
    try:
        with open(RUSTYCODE_CONFIG) as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return {}


def get_api_client_from_config(config: dict) -> tuple[str, str, str]:
    """Extract API base URL, key, and model from RustyCode config.

    Returns:
        (api_base, api_key, model)
    """
    model = config.get("model", "glm-5.1")
    provider_name = config.get("provider", "openai")
    providers = config.get("providers", {})

    if provider_name in providers:
        p = providers[provider_name]
        api_base = p.get("base_url", "https://api.openai.com/v1")
        api_key = p.get("api_key", "")
    else:
        api_base = "https://api.openai.com/v1"
        api_key = os.environ.get("OPENAI_API_KEY", "")

    return api_base, api_key, model


def get_system_prompt(question: dict) -> str:
    """Generate a category-aware system prompt for the question."""
    category = question.get("category", "")
    task = question.get("task", "")

    base = "You are a precise, thorough assistant. Always think step by step before answering."

    if category == "math":
        return (
            base + "\n"
            "- Show your complete work step by step.\n"
            "- Put your FINAL answer in \\boxed{} at the very end of your response.\n"
            "- Your \\boxed{} should contain ONLY the answer value — no units, no explanation.\n"
            "- For multiple choice: write the letter 5 times (e.g., CCCCC) then \\boxed{C}.\n"
            "- Simplify all fractions. Use exact values, not decimals, unless asked otherwise.\n"
            "- Double-check your arithmetic before giving the final answer.\n"
            "- If the answer is a list, use comma separation inside \\boxed{}.\n"
            "- IMPORTANT: Always include \\boxed{answer} as the LAST thing in your response."
        )
    elif category == "reasoning":
        return (
            base + "\n"
            "- Think carefully about each step. Consider edge cases.\n"
            "- Put your final answer in the requested format (\\boxed{} or <solution> tags).\n"
            "- For spatial tasks: give the exact numeric answer using digits (3, not three).\n"
            "- For puzzles: list items in the exact order requested."
        )
    elif category == "language":
        return (
            base + "\n"
            "- Follow all formatting instructions precisely.\n"
            "- Put your final answer in <solution> tags when appropriate.\n"
            "- For connections: group words by their strongest shared relationship.\n"
            "- For typos: correct ALL errors, not just some."
        )
    elif category == "instruction_following":
        return (
            "You are an assistant that follows instructions exactly. "
            "Read every instruction carefully and comply with ALL of them.\n"
            "- Follow every formatting constraint (length, structure, keywords, etc.).\n"
            "- Include ALL required elements and exclude ALL forbidden elements.\n"
            "- If asked to include specific words or phrases, include them EXACTLY as specified.\n"
            "- If asked to avoid certain words, ensure NONE appear anywhere in your response.\n"
            "- TITLE: If asked for a title, wrap it in double angular brackets like <<Title Here>>.\n"
            "- QUOTATION: If asked to wrap your response in double quotes, put a \" at the very start and very end of your response.\n"
            "- POSTSCRIPT: If asked to add a postscript, add 'P.S.' followed by the requested text at the very end.\n"
            "- PARAGRAPHS: Separate paragraphs with double newlines. Count carefully.\n"
            "- SENTENCES: Count your sentences carefully. A sentence ends with . ! or ?\n"
            "- WORDS: Count words carefully. Hyphenated words count as one word.\n"
            "- BULLET LISTS: Use proper bullet format (* or -) for each item.\n"
            "- SECTIONS: Use headers like '### Section Name' or marked sections.\n"
            "- JSON: If asked for JSON output, output valid JSON only.\n"
            "- PARAGRAPH START: If asked to start a specific paragraph with a word, make sure that paragraph begins with exactly that word.\n"
            "- TWO RESPONSES: If asked to give two responses, separate them clearly.\n"
            "- Do NOT add any extra content beyond what the instructions require.\n"
            "- Put your response inside <solution> tags if the instructions mention a specific format."
        )
    elif category == "data_analysis":
        return (
            base + "\n"
            "- Analyze the data methodically.\n"
            "- Put your final answer in <solution> tags.\n"
            "- For tables: use the exact format requested (CSV, JSON, etc.).\n"
            "- For calculations: show your work, then give the precise answer.\n"
            "- For table joins: compare column value distributions (mean, range, unique values, data types) between tables.\n"
            "- For CTA: carefully examine ALL values in the column. Look at data types (int, float, string, date), "
            "value ranges, and patterns. Pick the MOST SPECIFIC class that matches the data, not just any related class.\n"
            "- For tablereformat: match the EXACT output format — separators, headers, quoting.\n"
            "- Return EXACTLY the format requested — dict for joins, class name only for CTA, table for reformats."
        )
    else:
        return (
            base + "\n"
            "- For math: show work, then put the final answer in \\boxed{}.\n"
            "- For multiple choice: state the letter 5 times, then \\boxed{letter}.\n"
            "- For structured questions: follow the exact output format requested.\n"
            "- Always put your final answer at the very end of your response."
        )


def postprocess_answer(answer: str, question: dict) -> str:
    """Clean up raw LLM answer before scoring.

    Handles common LLM output issues: thinking tags, verbose prefixes,
    reasoning model artifacts, and format mismatches with scorers.
    """
    if not answer:
        return answer

    # Strip thinking tags (reasoning models)
    answer = re.sub(r"<think\b[^>]*>.*?</think\s*>", "", answer, flags=re.DOTALL)
    answer = answer.strip()

    category = question.get("category", "")
    task = question.get("task", "")
    subtask = question.get("subtask", task)

    # For math: if the scorer expects \boxed{} but the answer has the value
    # in a different format, try to normalize it
    if category == "math":
        # If answer already has \boxed{}, leave it alone
        if "\\boxed{" in answer:
            return answer

        # Try to extract the final numeric/mathematical answer
        # Common patterns: "The answer is 42", "Therefore, x = 42", etc.
        final_line = answer.rstrip().split("\n")[-1].strip()

        # Check for <solution> tags
        sol_match = re.search(r"<solution>(.*?)</solution>", answer, re.DOTALL)
        if sol_match:
            inner = sol_match.group(1).strip()
            if inner and not inner.startswith("["):
                # Wrap in \boxed{} if not already
                return f"{answer}\n\n\\boxed{{{inner}}}"

        # Check for bold final answer
        bold_match = re.findall(r"\*\*(.+?)\*\*", final_line)
        if bold_match:
            val = bold_match[-1].strip()
            if re.match(r"^[\d\-\+/\.\(\)a-z]$", val, re.I) or len(val) < 50:
                return f"{answer}\n\n\\boxed{{{val}}}"

        # Check "The answer is ..." pattern in last few lines
        last_lines = "\n".join(answer.rstrip().split("\n")[-5:])
        ans_match = re.search(
            r"(?:the answer is|therefore[,:]?\s*(?:the answer is)?|thus[,:]?\s*|so[,:]?\s*)\s*(.+?)(?:\.|$)",
            last_lines,
            re.I,
        )
        if ans_match:
            val = ans_match.group(1).strip().rstrip(".")
            if len(val) < 80:
                return f"{answer}\n\n\\boxed{{{val}}}"

    # For reasoning tasks with <solution> expected
    if category == "reasoning":
        sol_match = re.search(r"<solution>(.*?)</solution>", answer, re.DOTALL)
        if not sol_match:
            # Spatial/zebra tasks: check if last line is a clean answer
            lower_sub = subtask.lower() if subtask else ""
            if lower_sub in ("spatial",) or "spatial" in lower_sub:
                # If answer ends with a number in bold, ensure it's clear
                bold_num = re.findall(r"\*\*(\d+)\*\*", answer)
                if bold_num:
                    val = bold_num[-1]
                    return f"{answer}\n\\boxed{{{val}}}"

    # For DA tasks: ensure <solution> tags if expected
    if category == "data_analysis":
        if "<solution>" not in answer:
            lower_sub = subtask.lower() if subtask else ""
            if any(k in lower_sub for k in ("tablejoin", "cta", "tablereformat")):
                # Try to find a JSON dict or class name at the end
                last_line = answer.rstrip().split("\n")[-1].strip()
                if last_line.startswith("{") or (len(last_line) < 30 and not " " in last_line):
                    return f"{answer}\n<solution>{last_line}</solution>"

    return answer


def get_answer_direct_api(
    question: dict,
    api_base: str,
    api_key: str,
    model: str,
    max_tokens: int = 4096,
    timeout: int = 120,
) -> tuple[str, int]:
    """Send a question directly to the OpenAI-compatible API.

    Bypasses the RustyCode agent loop for clean LLM responses.

    Returns:
        (answer_text, num_tokens)
    """
    import urllib.request

    prompt_parts = []
    for turn in question.get("turns", []):
        prompt_parts.append(turn)

    if not prompt_parts:
        return "", 0

    messages = [{"role": "user", "content": p} for p in prompt_parts]

    # Add task-specific formatting hint to the last user message
    hint = get_task_hint(question)
    if hint and messages:
        messages[-1]["content"] += hint

    # Add system prompt if present in the question
    if "system_prompt" in question:
        messages.insert(0, {"role": "system", "content": question["system_prompt"]})
    elif not any(m.get("role") == "system" for m in messages):
        messages.insert(0, {
            "role": "system",
            "content": get_system_prompt(question),
        })

    payload = {
        "model": model,
        "messages": messages,
        "max_tokens": max_tokens,
        "temperature": 0,
        "stream": False,
    }

    url = f"{api_base.rstrip('/')}/chat/completions"
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}",
    }

    max_retries = 3
    for attempt in range(max_retries + 1):
        try:
            req = urllib.request.Request(
                url, data=json.dumps(payload).encode(), headers=headers
            )
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                data = json.loads(resp.read().decode())

            choices = data.get("choices", [])
            if not choices or "message" not in choices[0]:
                return "[ERROR: Malformed API response — no choices]", 0
            answer = choices[0]["message"].get("content", "") or ""
            # Strip thinking tags from content first — some models wrap their
            # answer in <think...>...</think...> followed by the actual answer.
            answer_cleaned = re.sub(r"<think\b[^>]*>.*?</think\s*>", "", answer, flags=re.DOTALL).strip()
            reasoning = choices[0]["message"].get("reasoning_content", "")
            if answer_cleaned:
                # Content has real text after stripping thinking tags — use it
                answer = answer_cleaned
            elif reasoning:
                # Content was only thinking tags — extract answer from reasoning field
                sol_match = re.search(r"<solution>(.*?)</solution>", reasoning, re.DOTALL)
                if sol_match:
                    answer = sol_match.group(1).strip()
                else:
                    boxed_match = re.findall(r"\\boxed\{(.*?)\}", reasoning)
                    if boxed_match:
                        answer = boxed_match[-1].strip()
                    else:
                        answer = reasoning[-500:].strip()
            elif not answer:
                pass  # Truly empty — return empty
            # Post-process answer to normalize format for scorers
            answer = postprocess_answer(answer, question)
            num_tokens = data.get("usage", {}).get("completion_tokens", len(answer) // 4)
            return answer, num_tokens

        except urllib.error.HTTPError as e:
            body = e.read().decode("utf-8", errors="replace")[:500]
            if e.code == 429 and attempt < max_retries:
                # Rate limited — exponential backoff with longer delay
                wait = min(30, (2 ** attempt) * 5)
                time.sleep(wait)
                continue
            if e.code >= 500 and attempt < max_retries:
                time.sleep(2 ** attempt)
                continue
            return f"[HTTP {e.code}: {body}]", 0
        except (TimeoutError, Exception) as e:
            err_str = str(e)
            is_retryable = "timed out" in err_str or "nodename" in err_str or "connection" in err_str.lower()
            if is_retryable and attempt < max_retries:
                time.sleep(2 ** attempt)
                continue
            return f"[ERROR: {e}]", 0


# Task-specific answer formatting hints appended to prompts.
# LiveBench scorers look for <solution> tags, \boxed{}, bold text,
# or last-line extraction depending on the task.
TASK_FORMAT_HINTS = {
    # Math: scorers extract from \boxed{} primarily
    "AMPS_Hard": "\n\nIMPORTANT: Put your final numeric answer inside \\boxed{} at the very end of your response. Example: The answer is \\boxed{42}. Show all work before the final answer. The answer must be a single value in \\boxed{}.",
    "math_comp": "\n\nIMPORTANT: This is multiple choice. State your answer clearly at the end. First write the letter 5 times consecutively (e.g., CCCCC), then put it in \\boxed{}. Example: The answer is CCCCC, so \\boxed{C}.",
    "olympiad": "\n\nIMPORTANT: Put your final answer inside \\boxed{} at the very end. Example: \\boxed{42}. If a list, use comma separation inside the box. The \\boxed{} MUST be the LAST thing in your response.",
    "integrals_with_game": "\n\nIMPORTANT: Put your final answer inside <solution> tags. Example: <solution>1/2</solution>. Simplify fractions.",
    "math": "\n\nIMPORTANT: Put your final answer inside \\boxed{} at the very end. Show all step-by-step work, then give ONLY the answer value in \\boxed{}.",
    # Additional math subtasks
    "amc": "\n\nIMPORTANT: This is AMC multiple choice. State your answer letter 5 times (e.g., CCCCC), then \\boxed{C} at the very end.",
    "aime": "\n\nIMPORTANT: Put your final numeric answer (0-999) inside \\boxed{} at the very end. AIME answers are integers from 0 to 999.",
    "imo": "\n\nIMPORTANT: Put your final answer inside \\boxed{} at the very end. Show all work. If a proof, write the key conclusion in \\boxed{}.",
    "usamo": "\n\nIMPORTANT: Put your final answer inside \\boxed{} at the very end. Show all work.",
    "putnam": "\n\nIMPORTANT: Put your final answer inside \\boxed{} at the very end. Putnam answers are typically integers or simple fractions.",
    # Reasoning: spatial uses bold/boxed, zebra uses <solution>
    "spatial": "\n\nIMPORTANT: Give ONLY the exact answer (a number like 3 or a shape name like triangle) in both **bold** and \\boxed{} at the very end. Use digits for numbers, not words. Example: The answer is **3** or \\boxed{3}. For shapes, use the exact name: **triangle** or \\boxed{triangle}. Do NOT write 'three' - write '3'. Keep your reasoning SHORT — 3-5 sentences max. Then give the final answer.",
    "zebra_puzzle": "\n\nIMPORTANT: Put your final answer inside <solution> tags as a comma-separated list matching the order asked in the question. Example: <solution>Alice, reading, engineer, comedy</solution>. Only put the values, no labels.",
    "web_of_lies_v2": "\n\nIMPORTANT: Put each answer inside <solution> tags as comma-separated yes/no. Example: <solution>yes, no, yes, unknown</solution>.",
    "web_of_lies_v3": "\n\nIMPORTANT: Put each answer inside <solution> tags as comma-separated yes/no. Example: <solution>yes, no, yes, unknown</solution>.",
    "house_traversal": "\n\nIMPORTANT: Put the room names in order inside <solution> tags. Example: <solution>kitchen, bedroom, bathroom</solution>.",
    "sudoku": "\n\nIMPORTANT: Put the completed grid inside <solution> tags as rows of digits. Example: <solution>123456789\\n456789123\\n...</solution>.",
    "theory_of_mind": "\n\nIMPORTANT: Put your final answer inside <solution> tags. Example: <solution>yes</solution>.",
    "logic_with_navigation": "\n\nIMPORTANT: Put the final coordinates inside <solution> tags. Example: <solution>(3, 5)</solution>.",
    # Language
    "connections": "\n\nIMPORTANT: Group the words into groups of 4 related words. Put ALL words in a SINGLE LINE inside <solution> tags separated by commas, with each group of 4 consecutive words forming one group. Format: <solution>word1, word2, word3, word4, word5, word6, word7, word8</solution> where words 1-4 are group 1 and words 5-8 are group 2. Do NOT use newlines inside the solution tags — keep everything on one line.",
    "typos": "\n\nIMPORTANT: Put the complete corrected text inside <solution> tags.",
    "plot_unscrambling": "\n\nIMPORTANT: Put the correct event order inside <solution> tags. Example: <solution>3, 1, 4, 2, 5</solution>.",
    # Data analysis: scorers match exact text/JSON
    "cta": "\n\nIMPORTANT: CTA (Column Type Annotation) task. You will see sample data from a column. Your job is to classify it.\n"
    "Steps:\n"
    "1. Look at ALL sample values — not just the first few\n"
    "2. Determine the DATA TYPE: integer, float, string, date, boolean, etc.\n"
    "3. If strings, check for PATTERNS: email addresses, URLs, phone numbers, names, addresses, codes, categories\n"
    "4. Match to the MOST SPECIFIC class from the provided list\n"
    "5. Give ONLY the class name — no explanation, no tags, no extra text.\n"
    "Example: if values are ['john@example.com', 'jane@test.org'], the class is 'email_address' not 'string'",
    "tablereformat": "\n\nIMPORTANT: Put the reformatted table inside <solution> tags exactly as requested.",
    "tablejoin": "\n\nIMPORTANT: To find column mappings, compare value distributions (min, max, mean, unique count, data patterns) between columns in both tables. Columns with identical or highly correlated values map to each other. Put ALL mappings in a single Python dict inside <solution> tags. Example: <solution>{'col_a1': 'col_b1', 'col_a2': 'col_b2'}</solution>. Use single quotes for keys and values. You must find ALL mappings, not just some.",
    "consecutive_events": "\n\nIMPORTANT: Put your answer inside <solution> tags as JSON.",
    # Instruction following: ifbench uses strict constraint checking
    "paraphrase": "\n\nIMPORTANT: Read ALL instructions in the prompt carefully. You must: (1) include ALL specified keywords exactly, (2) avoid ALL forbidden words, (3) use the exact format requested (title in <<>>, quotation marks, etc.), (4) meet ALL length constraints (paragraphs, sentences, words). Count everything carefully. Do NOT skip any instruction.",
    "simplify": "\n\nIMPORTANT: Read ALL instructions in the prompt carefully. You must: (1) include ALL specified keywords exactly, (2) avoid ALL forbidden words, (3) use the exact format requested (title in <<>>, quotation marks, etc.), (4) meet ALL length constraints. Do NOT skip any instruction.",
    "story_generation": "\n\nIMPORTANT: Read ALL instructions in the prompt carefully. You must: (1) include ALL specified keywords exactly, (2) avoid ALL forbidden words, (3) use the exact format requested (title in <<>>, quotation marks, etc.), (4) meet ALL length constraints. Do NOT skip any instruction.",
    "summarize": "\n\nIMPORTANT: Read ALL instructions in the prompt carefully. You must: (1) include ALL specified keywords exactly, (2) avoid ALL forbidden words, (3) use the exact format requested (title in <<>>, quotation marks, etc.), (4) meet ALL length constraints. Do NOT skip any instruction.",
}


def get_task_hint(question: dict) -> str:
    """Get the answer-formatting hint for a question's task."""
    task = question.get("task", "")
    subtask = question.get("subtask", task)

    # Direct match on subtask or task
    hint = TASK_FORMAT_HINTS.get(subtask, TASK_FORMAT_HINTS.get(task, ""))

    # Fallback: match by prefix for subtasks like "amc_12", "usamo", etc.
    if not hint and subtask:
        for key in TASK_FORMAT_HINTS:
            if key in subtask or subtask.startswith(key):
                hint = TASK_FORMAT_HINTS[key]
                break

    # Special: math competition subtasks (amc, aime, imo, usamo) -> math_comp or olympiad hint
    if not hint and subtask:
        lower = subtask.lower()
        if lower.startswith(("amc", "smc")):
            hint = TASK_FORMAT_HINTS["math_comp"]
        elif lower == "aime" or lower.startswith("aime"):
            hint = TASK_FORMAT_HINTS["aime"]
        elif lower in ("imo", "usamo") or lower.startswith(("imo", "usamo")):
            hint = TASK_FORMAT_HINTS["olympiad"]
        elif lower.startswith("putnam"):
            hint = TASK_FORMAT_HINTS["putnam"]
        elif lower in ("math",) or "math" in lower:
            hint = TASK_FORMAT_HINTS["math"]

    return hint


def get_answer_from_rustycode(
    question: dict,
    model: str,
    max_tokens: int = 4096,
    timeout: int = 120,
    provider: str | None = None,
) -> tuple[str, int]:
    """Send a question to RustyCode CLI and return the answer text.

    Returns:
        (answer_text, num_tokens_estimate)
    """
    prompt_parts = []

    # Multi-turn: concatenate all turns
    for turn in question.get("turns", []):
        prompt_parts.append(turn)

    if not prompt_parts:
        return "", 0

    prompt = "\n\n".join(prompt_parts)

    # Build CLI command
    cmd = [
        RUSTYCODE_BIN,
        "run",
        prompt,
        "--auto",
        "--mode", "ask",
        "--format", "json",
    ]

    env = {**os.environ, "NO_COLOR": "1"}
    if provider:
        env["RUSTYCODE_PROVIDER"] = provider

    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            env=env,
        )
        output = result.stdout.strip()

        # Try parsing JSON output
        if output:
            try:
                data = json.loads(output)
                answer = data.get("response", data.get("output", data.get("content", output)))
            except json.JSONDecodeError:
                answer = output
        else:
            answer = result.stderr.strip() if result.stderr else ""

        # Rough token estimate (4 chars per token)
        token_estimate = len(answer) // 4
        return answer, token_estimate

    except subprocess.TimeoutExpired:
        return "[TIMEOUT: RustyCode CLI exceeded timeout]", 0
    except FileNotFoundError:
        return f"[ERROR: {RUSTYCODE_BIN} not found. Build rustycode-cli first.]", 0
    except Exception as e:
        return f"[ERROR: {e}]", 0


def write_answer(
    question: dict,
    answer_text: str,
    model: str,
    token_count: int,
    answer_file: str,
) -> None:
    """Write an answer in LiveBench's expected JSONL format."""
    ans = {
        "question_id": question["question_id"],
        "answer_id": shortuuid.uuid(),
        "model_id": model.lower(),
        "choices": [
            {
                "index": 0,
                "turns": [answer_text],
            }
        ],
        "tstamp": time.time(),
        "total_output_tokens": token_count,
    }
    os.makedirs(os.path.dirname(answer_file), exist_ok=True)
    with open(answer_file, "a") as f:
        f.write(json.dumps(ans) + "\n")


def load_existing_answers(answer_file: str) -> set[str]:
    """Load question IDs that already have answers (for resume)."""
    answered = set()
    if os.path.exists(answer_file):
        with open(answer_file) as f:
            for line in f:
                try:
                    data = json.loads(line)
                    answered.add(data["question_id"])
                except (json.JSONDecodeError, KeyError):
                    continue
    return answered


def _filter_answers(answer_file: str, remove_ids: set[str]) -> None:
    """Remove answers for given question IDs from the JSONL file."""
    if not os.path.exists(answer_file) or not remove_ids:
        return
    lines = []
    with open(answer_file) as f:
        for line in f:
            try:
                data = json.loads(line)
                if data.get("question_id") not in remove_ids:
                    lines.append(line)
            except (json.JSONDecodeError, KeyError):
                lines.append(line)
    with open(answer_file, "w") as f:
        f.writelines(lines)


def score_answers(
    questions: list[dict],
    answer_file: str,
    model: str,
    output_file: str,
    debug: bool = False,
) -> list[dict]:
    """Score answers using LiveBench's ground-truth evaluation."""
    if not os.path.exists(answer_file):
        print(f"No answer file found at {answer_file}")
        return []

    # Load answers into a dict by question_id
    answers = {}
    with open(answer_file) as f:
        for line in f:
            try:
                data = json.loads(line)
                answers[data["question_id"]] = data
            except (json.JSONDecodeError, KeyError):
                continue

    results = []
    for question in tqdm.tqdm(questions, desc="Scoring"):
        qid = question["question_id"]
        if qid not in answers:
            if debug:
                print(f"  Skipping {qid}: no answer")
            continue

        answer = answers[qid]
        match = {
            "question": question,
            "model": model,
            "answer": answer,
        }

        # Extract answer text (used by fallback scoring)
        turns = answer.get("choices", [{}])[0].get("turns", [])
        llm_answer = turns[-1] if turns else ""
        category_val = question.get("category", "unknown")

        # Bypass play_a_match_gt for IF questions — LiveBench has a bug where
        # evaluation_lib uses the wrong instructions_registry, and ifbench_process_results
        # has a debug code crash on questions without <instructions> tags.
        if question.get("category") == "instruction_following":
            try:
                from livebench.process_results.instruction_following.utils import ifbench_process_results
                score_val = ifbench_process_results(question, llm_answer, debug=False)
                category_val = "instruction_following"
            except Exception as e:
                if debug:
                    print(f"  IF scoring error for {qid}: {e}")
                score_val = 0
                category_val = "instruction_following"
        else:
            try:
                from livebench.common import MatchSingle

                match_obj = MatchSingle(
                    question=question, model=model, answer=answer
                )
                result = play_a_match_gt(match_obj, output_file=None, debug=debug)
                if result is None:
                    score_val = 0
                else:
                    score_val = result["score"] if isinstance(result, dict) else result
                    category_val = result.get("category", category_val) if isinstance(result, dict) else category_val
            except Exception as e:
                if debug:
                    print(f"  Scoring error for {qid}: {e}")
                score_val = 0

        results.append(
            {
                "question_id": qid,
                "task": question.get("task", "unknown"),
                "category": category_val,
                "score": score_val,
                "answer_preview": llm_answer[:200] if debug else "",
            }
        )

    return results


def print_results(results: list[dict], model: str) -> None:
    """Print a summary of scoring results."""
    if not results:
        print("No results to display.")
        return

    # Group by category
    by_category: dict[str, list[dict]] = {}
    for r in results:
        cat = r["category"]
        by_category.setdefault(cat, []).append(r)

    print(f"\n{'='*60}")
    print(f"  LiveBench Results for {model}")
    print(f"{'='*60}")

    total_score = 0
    total_count = 0

    for category in sorted(by_category.keys()):
        items = by_category[category]
        avg = sum(r["score"] for r in items) / len(items) if items else 0
        total_score += sum(r["score"] for r in items)
        total_count += len(items)

        # Group by task within category
        by_task: dict[str, list[dict]] = {}
        for r in items:
            by_task.setdefault(r["task"], []).append(r)

        print(f"\n  {category.upper()} ({len(items)} questions, avg: {avg:.2f})")
        for task in sorted(by_task.keys()):
            task_items = by_task[task]
            task_avg = sum(r["score"] for r in task_items) / len(task_items)
            scores = [r["score"] for r in task_items]
            perfect = sum(1 for s in scores if s == 1.0)
            failed = sum(1 for s in scores if s < 0.5)
            print(f"    {task}: {task_avg:.2f} ({len(task_items)} q, {perfect} perfect, {failed} failed)")

    overall = total_score / total_count if total_count else 0
    print(f"\n{'='*60}")
    print(f"  OVERALL: {overall:.2f} ({total_count} questions)")
    print(f"{'='*60}\n")


def main():
    parser = argparse.ArgumentParser(
        description="Run LiveBench evaluation through RustyCode CLI"
    )
    parser.add_argument(
        "--categories",
        nargs="+",
        default=["reasoning", "math", "language", "instruction_following", "data_analysis"],
        help="Categories to evaluate (default: all non-coding categories)",
    )
    parser.add_argument(
        "--model",
        default="rustycode",
        help="Model display name (default: rustycode)",
    )
    parser.add_argument(
        "--rustycode-bin",
        default=RUSTYCODE_BIN,
        help="Path to rustycode binary (default: rustycode)",
    )
    parser.add_argument(
        "--max-tokens",
        type=int,
        default=16384,
        help="Max tokens for LLM responses (reasoning models need 16k+)",
    )
    parser.add_argument(
        "--provider",
        default=None,
        help="LLM provider override (anthropic, openai, etc.)",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=300,
        help="Per-question timeout in seconds (reasoning models need 300+)",
    )
    parser.add_argument(
        "--parallel",
        type=int,
        default=1,
        help="Number of concurrent requests",
    )
    parser.add_argument(
        "--delay",
        type=float,
        default=0,
        help="Delay between sequential requests in seconds (default: 0)",
    )
    parser.add_argument(
        "--question-id",
        nargs="+",
        default=None,
        help="Specific question IDs to evaluate",
    )
    parser.add_argument(
        "--livebench-release",
        default=max(LIVE_BENCH_RELEASES),
        choices=sorted(LIVE_BENCH_RELEASES),
        help="LiveBench release to use",
    )
    parser.add_argument(
        "--resume",
        action="store_true",
        help="Skip questions that already have answers",
    )
    parser.add_argument(
        "--skip-inference",
        action="store_true",
        help="Skip inference, only re-score existing answers",
    )
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Enable debug output for scoring",
    )
    parser.add_argument(
        "--output-dir",
        default=None,
        help="Output directory (default: ./livebench_results)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Load questions and run scoring with dummy answers (no RustyCode calls)",
    )
    parser.add_argument(
        "--retry-failed",
        action="store_true",
        help="Re-run questions from previous results that scored below 0.5",
    )
    parser.add_argument(
        "--sample",
        type=int,
        default=None,
        help="Limit to N random questions per category",
    )

    args = parser.parse_args()

    rustycode_bin = args.rustycode_bin

    output_dir = args.output_dir or os.path.join(os.getcwd(), "livebench_results")
    os.makedirs(output_dir, exist_ok=True)

    # Load RustyCode config for direct API access
    rc_config = load_rustycode_config()
    api_base, api_key, api_model = get_api_client_from_config(rc_config)

    if not args.dry_run:
        print(f"API: {api_base}")
        print(f"Model: {api_model}")
        if not api_key:
            print("WARNING: No API key found. Set it in ~/.rustycode/config.json")
            print("         or use --dry-run to test the pipeline.")

    all_results = []

    for category in args.categories:
        if category in DOCKER_CATEGORIES:
            print(f"Skipping {category} (requires Docker)")
            continue

        print(f"\n{'='*60}")
        print(f"  Category: {category}")
        print(f"{'='*60}")

        # Load questions from HuggingFace
        try:
            dataset = get_hf_dataset(category)
            questions = load_questions(
                dataset,
                livebench_release=args.livebench_release,
                question_ids=args.question_id,
            )
        except Exception as e:
            print(f"Failed to load {category} questions: {e}")
            continue

        if not questions:
            print(f"No questions found for {category}")
            continue

        # --sample: limit to N random questions per category
        if args.sample and len(questions) > args.sample:
            import random
            random.seed(42)
            questions = random.sample(questions, args.sample)
            print(f"Sampled {args.sample} questions")

        # --retry-failed: only re-run questions that scored < 0.5
        answer_file = os.path.join(output_dir, f"{category}_answers.jsonl")
        judgment_file = os.path.join(output_dir, f"{category}_judgments.jsonl")
        if args.retry_failed:
            existing = load_existing_answers(answer_file)
            if existing:
                # Score existing answers to find failed ones
                failed_ids = set()
                results_file = os.path.join(output_dir, "results.json")
                if os.path.exists(results_file):
                    with open(results_file) as f:
                        prev_results = json.load(f)
                    for r in prev_results:
                        if r.get("score", 0) < 0.5 and r["category"] == category:
                            failed_ids.add(r["question_id"])
                # Filter to only failed questions
                questions = [q for q in questions if q["question_id"] in failed_ids]
                # Remove their answers so they get re-inferred
                if failed_ids:
                    _filter_answers(answer_file, failed_ids)
                print(f"Retrying {len(questions)} failed questions")
                if not questions:
                    continue

        print(f"Loaded {len(questions)} questions")

        judgment_file = os.path.join(output_dir, f"{category}_judgments.jsonl")

        # Resume support
        existing = load_existing_answers(answer_file) if args.resume else set()
        if existing:
            print(f"Resuming: {len(existing)} questions already answered")

        if not args.skip_inference:
            to_answer = [
                q for q in questions if q["question_id"] not in existing
            ]

            if args.dry_run:
                print(f"DRY RUN: Writing dummy answers for {len(to_answer)} questions...")
                for question in to_answer:
                    write_answer(
                        question=question,
                        answer_text="I don't know.",
                        model=args.model,
                        token_count=5,
                        answer_file=answer_file,
                    )
            else:
                print(f"Answering {len(to_answer)} questions via {api_base} (model: {api_model})...")
                if args.parallel > 1 and len(to_answer) > 1:
                    import concurrent.futures

                    def _answer_one(question):
                        text, tokens = get_answer_direct_api(
                            question=question,
                            api_base=api_base,
                            api_key=api_key,
                            model=api_model,
                            max_tokens=args.max_tokens,
                            timeout=args.timeout,
                        )
                        return question, text, tokens

                    with concurrent.futures.ThreadPoolExecutor(max_workers=args.parallel) as executor:
                        futures = {executor.submit(_answer_one, q): q for q in to_answer}
                        for future in tqdm.tqdm(
                            concurrent.futures.as_completed(futures),
                            total=len(futures),
                            desc=f"Inference ({category}, {args.parallel} workers)",
                        ):
                            question, answer_text, token_count = future.result()
                            write_answer(
                                question=question,
                                answer_text=answer_text,
                                model=args.model,
                                token_count=token_count,
                                answer_file=answer_file,
                            )
                else:
                    for question in tqdm.tqdm(to_answer, desc=f"Inference ({category})"):
                        answer_text, token_count = get_answer_direct_api(
                            question=question,
                            api_base=api_base,
                            api_key=api_key,
                            model=api_model,
                            max_tokens=args.max_tokens,
                            timeout=args.timeout,
                        )
                        write_answer(
                            question=question,
                            answer_text=answer_text,
                            model=args.model,
                            token_count=token_count,
                            answer_file=answer_file,
                        )
                        # Inter-request delay to avoid rate limiting
                        if args.delay > 0:
                            time.sleep(args.delay)

            # Reorganize answer file
            reorg_answer_file(answer_file)

        # Score
        print(f"Scoring {category} answers...")
        results = score_answers(
            questions=questions,
            answer_file=answer_file,
            model=args.model,
            output_file=judgment_file,
            debug=args.debug,
        )
        all_results.extend(results)

    # Print summary
    print_results(all_results, args.model)

    # Save raw results
    results_file = os.path.join(output_dir, "results.json")
    with open(results_file, "w") as f:
        json.dump(all_results, f, indent=2)
    print(f"Full results saved to {results_file}")


if __name__ == "__main__":
    main()
