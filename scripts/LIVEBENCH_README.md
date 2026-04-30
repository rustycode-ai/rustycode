# LiveBench Adapter for RustyCode

Evaluates RustyCode's LLM performance against [LiveBench](https://github.com/livebench/livebench) — an ICLR 2025 Spotlight LLM benchmark with objective ground-truth scoring.

## Setup

```bash
# 1. Clone LiveBench (sibling to rustycode)
cd /Users/nat/dev
git clone https://github.com/livebench/livebench

# 2. Install LiveBench in a venv
cd livebench
python3 -m venv .venv
.venv/bin/pip install -e . 'setuptools<82'

# 3. Build RustyCode CLI
cd /Users/nat/dev/rustycode
cargo build --release -p rustycode-cli

# 4. Set your API key
export ANTHROPIC_API_KEY="sk-ant-..."
```

## Quick Start

```bash
# Run all non-coding categories (782 questions)
./scripts/livebench_run.sh

# Run specific categories
./scripts/livebench_run.sh reasoning math

# Dry-run (test pipeline without calling RustyCode)
./scripts/livebench_run.sh --dry-run reasoning

# Resume interrupted run
./scripts/livebench_run.sh --resume math

# Override binary path
RUSTYCODE_BIN=./target/debug/rustycode-cli ./scripts/livebench_run.sh reasoning

# Run with debug scoring output
./scripts/livebench_run.sh --debug --question-id <id> reasoning
```

## Available Categories

| Category | Tasks | Questions | Requires Docker |
|----------|-------|-----------|-----------------|
| reasoning | spatial, zebra_puzzle | 100 | No |
| math | AMPS_Hard, math_comp, olympiad | 182 | No |
| language | connections | 50 | No |
| instruction_following | paraphrase, simplify, story_generation, summarize | 200 | No |
| data_analysis | cta, tablejoin, tablereformat | 150 | No |
| coding | LCB_generation, code_generation, agentic_coding | varies | Yes |

## How It Works

1. Loads questions from HuggingFace datasets
2. Calls `rustycode run "<prompt>" --auto --mode ask --format json` per question
3. Writes answers in LiveBench's JSONL format
4. Scores against ground truth using LiveBench's objective evaluators
5. Reports per-category and per-task averages

## Output

Results are written to `./livebench_results/` by default:

```
livebench_results/
├── reasoning_answers.jsonl      # Raw answers
├── reasoning_judgments.jsonl    # Scoring results
├── math_answers.jsonl
├── math_judgments.jsonl
└── results.json                 # Combined results
```

Example output:

```
============================================================
  LiveBench Results for rustycode
============================================================

  MATH (182 questions, avg: 0.42)
    AMPS_Hard: 0.35 (100 q)
    math_comp: 0.52 (46 q)
    olympiad: 0.44 (36 q)

  REASONING (100 questions, avg: 0.58)
    spatial: 0.62 (50 q)
    zebra_puzzle: 0.54 (50 q)

============================================================
  OVERALL: 0.48 (282 questions)
============================================================
```

## CLI Options

```
--categories          Categories to evaluate (default: reasoning math language instruction_following)
--model               Model display name (default: rustycode)
--provider            LLM provider override (anthropic, openai)
--timeout             Per-question timeout in seconds (default: 120)
--max-tokens          Max tokens per response (default: 4096)
--resume              Skip already-answered questions
--skip-inference      Only re-score existing answers
--dry-run             Use dummy answers to test pipeline
--question-id         Evaluate specific question IDs
--livebench-release   LiveBench release date (default: latest)
--debug               Enable debug output
--output-dir          Output directory (default: ./livebench_results)
```
