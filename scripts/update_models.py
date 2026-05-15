#!/usr/bin/env python3
"""Generate model_catalog.rs from LiteLLM's model_prices_and_context_window.json.

Fetches the upstream JSON, filters chat models for providers we support,
merges with custom hardcoded models, and generates a committed Rust source file.

Usage:
    python3 scripts/update_models.py [--output PATH]

Output file defaults to crates/rustycode-providers/src/model_catalog.rs
"""

import json
import sys
import urllib.request
from pathlib import Path

URL = "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json"
REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_OUTPUT = REPO_ROOT / "crates/rustycode-providers/src/model_catalog.rs"

# ---------------------------------------------------------------------------
# Provider mapping: LiteLLM provider → our internal provider_id
# ---------------------------------------------------------------------------
PROVIDER_MAP = {
    "openai": "openai",
    "anthropic": "anthropic",
    "azure": "azure",
    "gemini": "gemini",
    "vertex_ai": "vertex",
    "mistral": "mistral",
    "perplexity": "perplexity",
    "openrouter": "openrouter",
    "together_ai": "together",
    "bedrock": "bedrock",
}

# Providers we want to include from LiteLLM
WANTED_PROVIDERS = set(PROVIDER_MAP.keys())

# ---------------------------------------------------------------------------
# Models we want to keep — base names only (no dated snapshots)
# Keys are (provider_prefix, base_name) tuples.
# provider_prefix is the part before '/' in the LiteLLM key, or the model name itself.
# ---------------------------------------------------------------------------
WANTED_MODELS = {
    # OpenAI
    "gpt-5.5", "gpt-5.4", "gpt-5.4-mini",
    "gpt-5.2", "gpt-5.1", "gpt-5.1-mini",
    "gpt-4.1", "gpt-4.1-mini", "gpt-4.1-nano",
    "gpt-4o", "gpt-4o-mini", "gpt-4-turbo",
    "o4-mini", "o3", "o3-mini", "o1", "o1-mini",
    # Anthropic
    "claude-opus-4-7", "claude-opus-4-6",
    "claude-sonnet-4-6", "claude-sonnet-4-5",
    "claude-haiku-4-5",
    # Gemini
    "gemini-2.5-pro", "gemini-2.5-flash", "gemini-2.5-flash-lite",
    "gemini-2.0-flash",
    # Mistral
    "mistral-large-latest", "mistral-medium-latest", "mistral-small-latest",
    "codestral-latest", "open-mistral-nemo",
    # Perplexity
    "sonar-pro", "sonar", "sonar-reasoning", "sonar-reasoning-pro",
    # OpenRouter — provider-prefixed models
    "anthropic/claude-sonnet-4-6", "anthropic/claude-opus-4-7",
    "openai/gpt-4o", "openai/o3-mini",
    "google/gemini-2.5-pro", "google/gemini-2.5-flash",
    "google/gemini-2.5-flash:free",
    "deepseek/deepseek-chat",
    "meta-llama/llama-3.3-70b-instruct",
}

# ---------------------------------------------------------------------------
# Custom models NOT in LiteLLM — hardcoded entries
# ---------------------------------------------------------------------------
CUSTOM_MODELS = [
    # Zhipu / z.ai GLM models
    {
        "id": "glm-5.1",
        "provider": "zhipu",
        "context_window": 200_000,
        "max_output": 131_072,
        "input_cost_per_1m": 5.0,
        "output_cost_per_1m": 20.0,
        "vision": True,
        "tools": True,
        "reasoning": True,
        "caching": False,
    },
    {
        "id": "glm-5",
        "provider": "zhipu",
        "context_window": 200_000,
        "max_output": 16_384,
        "input_cost_per_1m": 3.0,
        "output_cost_per_1m": 15.0,
        "vision": True,
        "tools": True,
        "reasoning": True,
        "caching": False,
    },
    {
        "id": "glm-5-turbo",
        "provider": "zhipu",
        "context_window": 200_000,
        "max_output": 16_384,
        "input_cost_per_1m": 2.0,
        "output_cost_per_1m": 8.0,
        "vision": True,
        "tools": True,
        "reasoning": True,
        "caching": False,
    },
    {
        "id": "glm-4.7-flash",
        "provider": "zhipu",
        "context_window": 128_000,
        "max_output": 8_192,
        "input_cost_per_1m": 0.1,
        "output_cost_per_1m": 0.1,
        "vision": False,
        "tools": True,
        "reasoning": False,
        "caching": False,
    },
    # Kimi / Moonshot (China)
    {
        "id": "kimi-k2",
        "provider": "kimi-cn",
        "context_window": 200_000,
        "max_output": 8_192,
        "input_cost_per_1m": 3.0,
        "output_cost_per_1m": 15.0,
        "vision": True,
        "tools": True,
        "reasoning": False,
        "caching": False,
    },
    {
        "id": "kimi-latest",
        "provider": "kimi-cn",
        "context_window": 200_000,
        "max_output": 8_192,
        "input_cost_per_1m": 3.0,
        "output_cost_per_1m": 15.0,
        "vision": True,
        "tools": True,
        "reasoning": False,
        "caching": False,
    },
    # Kimi / Moonshot (Global)
    {
        "id": "kimi-k2",
        "provider": "kimi-global",
        "context_window": 200_000,
        "max_output": 8_192,
        "input_cost_per_1m": 3.0,
        "output_cost_per_1m": 15.0,
        "vision": True,
        "tools": True,
        "reasoning": False,
        "caching": False,
    },
    {
        "id": "kimi-latest",
        "provider": "kimi-global",
        "context_window": 200_000,
        "max_output": 8_192,
        "input_cost_per_1m": 3.0,
        "output_cost_per_1m": 15.0,
        "vision": True,
        "tools": True,
        "reasoning": False,
        "caching": False,
    },
    # Alibaba / DashScope (China)
    {
        "id": "qwen-max",
        "provider": "alibaba-cn",
        "context_window": 128_000,
        "max_output": 8_192,
        "input_cost_per_1m": 2.0,
        "output_cost_per_1m": 6.0,
        "vision": True,
        "tools": True,
        "reasoning": False,
        "caching": False,
    },
    {
        "id": "qwen-coder-plus",
        "provider": "alibaba-cn",
        "context_window": 128_000,
        "max_output": 8_192,
        "input_cost_per_1m": 1.0,
        "output_cost_per_1m": 3.0,
        "vision": False,
        "tools": True,
        "reasoning": False,
        "caching": False,
    },
    # Alibaba / DashScope (Global)
    {
        "id": "qwen-max",
        "provider": "alibaba-global",
        "context_window": 128_000,
        "max_output": 8_192,
        "input_cost_per_1m": 2.0,
        "output_cost_per_1m": 6.0,
        "vision": True,
        "tools": True,
        "reasoning": False,
        "caching": False,
    },
    {
        "id": "qwen-coder-plus",
        "provider": "alibaba-global",
        "context_window": 128_000,
        "max_output": 8_192,
        "input_cost_per_1m": 1.0,
        "output_cost_per_1m": 3.0,
        "vision": False,
        "tools": True,
        "reasoning": False,
        "caching": False,
    },
    # GitHub Copilot (free via subscription)
    {
        "id": "gpt-5.5-copilot",
        "provider": "copilot",
        "context_window": 1_000_000,
        "max_output": 32_768,
        "input_cost_per_1m": 0.0,
        "output_cost_per_1m": 0.0,
        "vision": True,
        "tools": True,
        "reasoning": True,
        "caching": False,
    },
    {
        "id": "gpt-5.4-copilot",
        "provider": "copilot",
        "context_window": 1_000_000,
        "max_output": 32_768,
        "input_cost_per_1m": 0.0,
        "output_cost_per_1m": 0.0,
        "vision": True,
        "tools": True,
        "reasoning": True,
        "caching": False,
    },
    {
        "id": "claude-sonnet-4-6-copilot",
        "provider": "copilot",
        "context_window": 200_000,
        "max_output": 16_384,
        "input_cost_per_1m": 0.0,
        "output_cost_per_1m": 0.0,
        "vision": True,
        "tools": True,
        "reasoning": True,
        "caching": False,
    },
    # Ollama (local, free)
    {
        "id": "llama3",
        "provider": "ollama",
        "context_window": 128_000,
        "max_output": 4_096,
        "input_cost_per_1m": 0.0,
        "output_cost_per_1m": 0.0,
        "vision": False,
        "tools": False,
        "reasoning": False,
        "caching": False,
    },
    {
        "id": "qwen2.5-coder",
        "provider": "ollama",
        "context_window": 32_768,
        "max_output": 4_096,
        "input_cost_per_1m": 0.0,
        "output_cost_per_1m": 0.0,
        "vision": False,
        "tools": False,
        "reasoning": False,
        "caching": False,
    },
    {
        "id": "mistral",
        "provider": "ollama",
        "context_window": 32_000,
        "max_output": 4_096,
        "input_cost_per_1m": 0.0,
        "output_cost_per_1m": 0.0,
        "vision": False,
        "tools": False,
        "reasoning": False,
        "caching": False,
    },
    # LiteRT-LM (local, free)
    {
        "id": "gemma-4-e2b-it",
        "provider": "litert-lm",
        "context_window": 8_192,
        "max_output": 4_096,
        "input_cost_per_1m": 0.0,
        "output_cost_per_1m": 0.0,
        "vision": False,
        "tools": False,
        "reasoning": False,
        "caching": False,
    },
    {
        "id": "gemma-4-e4b-it",
        "provider": "litert-lm",
        "context_window": 8_192,
        "max_output": 4_096,
        "input_cost_per_1m": 0.0,
        "output_cost_per_1m": 0.0,
        "vision": False,
        "tools": False,
        "reasoning": False,
        "caching": False,
    },
    {
        "id": "gemma3-1b",
        "provider": "litert-lm",
        "context_window": 8_192,
        "max_output": 4_096,
        "input_cost_per_1m": 0.0,
        "output_cost_per_1m": 0.0,
        "vision": False,
        "tools": False,
        "reasoning": False,
        "caching": False,
    },
    {
        "id": "gemma-3n-e2b",
        "provider": "litert-lm",
        "context_window": 8_192,
        "max_output": 4_096,
        "input_cost_per_1m": 0.0,
        "output_cost_per_1m": 0.0,
        "vision": False,
        "tools": False,
        "reasoning": False,
        "caching": False,
    },
    {
        "id": "gemma-3n-e4b",
        "provider": "litert-lm",
        "context_window": 8_192,
        "max_output": 4_096,
        "input_cost_per_1m": 0.0,
        "output_cost_per_1m": 0.0,
        "vision": False,
        "tools": False,
        "reasoning": False,
        "caching": False,
    },
    {
        "id": "phi-4-mini",
        "provider": "litert-lm",
        "context_window": 8_192,
        "max_output": 4_096,
        "input_cost_per_1m": 0.0,
        "output_cost_per_1m": 0.0,
        "vision": False,
        "tools": False,
        "reasoning": False,
        "caching": False,
    },
    {
        "id": "qwen2.5-1.5b",
        "provider": "litert-lm",
        "context_window": 8_192,
        "max_output": 4_096,
        "input_cost_per_1m": 0.0,
        "output_cost_per_1m": 0.0,
        "vision": False,
        "tools": False,
        "reasoning": False,
        "caching": False,
    },
    {
        "id": "functiongemma-270m",
        "provider": "litert-lm",
        "context_window": 4_096,
        "max_output": 2_048,
        "input_cost_per_1m": 0.0,
        "output_cost_per_1m": 0.0,
        "vision": False,
        "tools": True,
        "reasoning": False,
        "caching": False,
    },
    # Groq (high-speed inference)
    {
        "id": "llama-3.1-70b-versatile",
        "provider": "groq",
        "context_window": 128_000,
        "max_output": 8_192,
        "input_cost_per_1m": 0.59,
        "output_cost_per_1m": 0.79,
        "vision": False,
        "tools": True,
        "reasoning": False,
        "caching": False,
    },
    {
        "id": "llama3-70b-8192",
        "provider": "groq",
        "context_window": 8_192,
        "max_output": 8_192,
        "input_cost_per_1m": 0.59,
        "output_cost_per_1m": 0.79,
        "vision": False,
        "tools": True,
        "reasoning": False,
        "caching": False,
    },
    # Google Vertex AI Gemini models
    {
        "id": "gemini-2.5-pro",
        "provider": "vertex",
        "context_window": 1_048_576,
        "max_output": 65_536,
        "input_cost_per_1m": 1.25,
        "output_cost_per_1m": 10.0,
        "vision": True,
        "tools": True,
        "reasoning": True,
        "caching": True,
    },
    {
        "id": "gemini-2.5-flash",
        "provider": "vertex",
        "context_window": 1_048_576,
        "max_output": 65_536,
        "input_cost_per_1m": 0.3,
        "output_cost_per_1m": 2.5,
        "vision": True,
        "tools": True,
        "reasoning": True,
        "caching": True,
    },
    {
        "id": "gemini-2.5-flash-lite",
        "provider": "vertex",
        "context_window": 1_048_576,
        "max_output": 65_536,
        "input_cost_per_1m": 0.1,
        "output_cost_per_1m": 0.4,
        "vision": True,
        "tools": True,
        "reasoning": False,
        "caching": True,
    },
]


def fetch_json(url: str) -> dict:
    """Fetch the LiteLLM JSON."""
    print(f"Fetching {url} ...")
    req = urllib.request.Request(url, headers={"User-Agent": "rustycode-update-models/1.0"})
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.loads(resp.read())


def extract_models(data: dict) -> list[dict]:
    """Extract wanted models from the LiteLLM JSON."""
    models = []
    seen_ids = set()

    for key, val in data.items():
        if val.get("mode") != "chat":
            continue

        litellm_provider = val.get("litellm_provider", "")
        if litellm_provider not in WANTED_PROVIDERS:
            continue

        # Parse the key: may be "provider/model-name" or just "model-name"
        if "/" in key:
            parts = key.split("/", 1)
            # For openrouter, keep the full provider/model key
            if litellm_provider == "openrouter":
                model_id = key
            else:
                model_id = parts[-1]
        else:
            model_id = key

        # Strip date suffixes like "-2024-05-13", "-20250514"
        # but keep "-4-6", "-4-7", "-mini" etc.
        base_id = model_id
        # Remove date-like suffixes: -YYYY-MM-DD or -YYYYMMDD
        import re
        base_id = re.sub(r"-\d{4}-\d{2}-\d{2}$", "", base_id)
        base_id = re.sub(r"-\d{8}$", "", base_id)
        # Also strip "-preview", "-native-audio-preview-*", etc.
        base_id = re.sub(r"-preview.*$", "", base_id)
        base_id = re.sub(r"-tts$", "", base_id)
        base_id = re.sub(r"-search.*$", "", base_id)
        base_id = re.sub(r"-online$", "", base_id)
        base_id = re.sub(r"-12-\d{4}$", "", base_id)  # gemini date suffix
        base_id = re.sub(r"-06-17$", "", base_id)  # gemini date suffix
        base_id = re.sub(r"-250\d$", "", base_id)  # mistral date suffix
        base_id = re.sub(r"-2407$", "", base_id)  # nemo date suffix
        base_id = re.sub(r"-2512$", "", base_id)  # date suffix
        base_id = re.sub(r"-chat-latest$", "", base_id)

        # Check if this base_id or model_id is in our wanted set
        matched_id = None
        candidates = [base_id, model_id]
        # For openrouter, also try matching without the "openrouter/" prefix
        # e.g., "openrouter/anthropic/claude-sonnet-4-6" → "anthropic/claude-sonnet-4-6"
        if litellm_provider == "openrouter" and "/" in model_id:
            candidates.append(model_id.split("/", 1)[1])
        for candidate in candidates:
            for wanted in WANTED_MODELS:
                if candidate == wanted:
                    matched_id = wanted
                    break
                if candidate.startswith(wanted):
                    matched_id = wanted
                    break
            if matched_id:
                break

        if matched_id is None:
            continue

        provider_id = PROVIDER_MAP.get(litellm_provider, litellm_provider)

        # Dedup: use matched_id + provider as unique key
        dedup_key = (matched_id, provider_id)
        if dedup_key in seen_ids:
            continue
        seen_ids.add(dedup_key)

        max_input = val.get("max_input_tokens") or val.get("max_tokens") or 128_000
        max_output = val.get("max_output_tokens") or val.get("max_tokens") or 4_096

        input_cost = val.get("input_cost_per_token", 0) * 1_000_000
        output_cost = val.get("output_cost_per_token", 0) * 1_000_000

        models.append({
            "id": matched_id,
            "provider": provider_id,
            "context_window": max_input,
            "max_output": max_output,
            "input_cost_per_1m": round(input_cost, 4),
            "output_cost_per_1m": round(output_cost, 4),
            "vision": bool(val.get("supports_vision")),
            "tools": bool(val.get("supports_function_calling")),
            "reasoning": bool(val.get("supports_reasoning")),
            "caching": bool(val.get("supports_prompt_caching")),
        })

    return models


def generate_rust(models: list[dict]) -> str:
    """Generate the Rust source file."""
    # Sort by provider then id
    models.sort(key=lambda m: (m["provider"], m["id"]))

    header = '''//! GENERATED by scripts/update_models.py — DO NOT EDIT MANUALLY
//!
//! Model catalog auto-generated from LiteLLM's model_prices_and_context_window.json.
//! To refresh: python3 scripts/update_models.py
//!
//! Source: https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json

/// A single model entry in the catalog.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct ModelEntry {
    /// Model identifier (e.g., "claude-sonnet-4-6", "gpt-4o")
    pub id: &'static str,
    /// Provider identifier (e.g., "anthropic", "openai")
    pub provider: &'static str,
    /// Maximum context window in tokens
    pub context_window: usize,
    /// Maximum output tokens
    pub max_output: usize,
    /// Cost per 1M input tokens in USD
    pub input_cost_per_1m: f64,
    /// Cost per 1M output tokens in USD
    pub output_cost_per_1m: f64,
    /// Whether the model supports vision/image inputs
    pub supports_vision: bool,
    /// Whether the model supports tool/function calling
    pub supports_tools: bool,
    /// Whether the model is a reasoning model
    pub supports_reasoning: bool,
    /// Whether the model supports prompt caching
    pub supports_caching: bool,
}

/// Default context window when a model is not in the catalog.
pub const DEFAULT_CONTEXT_WINDOW: usize = 100_000;

/// Default max output tokens when not specified.
pub const DEFAULT_MAX_OUTPUT: usize = 4_096;

/// The full model catalog, sorted by provider then model id.
pub static MODEL_CATALOG: &[ModelEntry] = &[
'''

    entries = []
    for m in models:
        b = "true" if m["vision"] else "false"
        t = "true" if m["tools"] else "false"
        r = "true" if m["reasoning"] else "false"
        c = "true" if m["caching"] else "false"
        entry = (
            f'    ModelEntry {{\n'
            f'        id: "{m["id"]}",\n'
            f'        provider: "{m["provider"]}",\n'
            f'        context_window: {m["context_window"]:_},\n'
            f'        max_output: {m["max_output"]:_},\n'
            f'        input_cost_per_1m: {m["input_cost_per_1m"]},\n'
            f'        output_cost_per_1m: {m["output_cost_per_1m"]},\n'
            f'        supports_vision: {b},\n'
            f'        supports_tools: {t},\n'
            f'        supports_reasoning: {r},\n'
            f'        supports_caching: {c},\n'
            f'    }},'
        )
        entries.append(entry)

    footer = '''
];

/// Iterate over all catalog entries.
pub fn catalog() -> impl Iterator<Item = &'static ModelEntry> {
    MODEL_CATALOG.iter()
}

/// Look up a model by its ID.
///
/// Uses a 3-tier matching strategy:
/// 1. Exact match on `id`
/// 2. Strip provider prefix (e.g., "zai-coding-plan/glm-5" → "glm-5")
/// 3. Prefix match (e.g., "claude-sonnet-4-6-20250514" matches "claude-sonnet-4-6")
pub fn lookup(model_id: &str) -> Option<&'static ModelEntry> {
    // 1. Exact match
    if let Some(entry) = MODEL_CATALOG.iter().find(|e| e.id == model_id) {
        return Some(entry);
    }

    // 2. Strip provider prefix (e.g., "zai-coding-plan/glm-5" → "glm-5")
    if let Some(short_id) = model_id.rsplit('/').next() {
        if short_id != model_id {
            if let Some(entry) = MODEL_CATALOG.iter().find(|e| e.id == short_id) {
                return Some(entry);
            }
        }
    }

    // 3. Prefix match (e.g., "claude-sonnet-4-6-20250514" matches "claude-sonnet-4-6")
    MODEL_CATALOG
        .iter()
        .find(|e| model_id.starts_with(e.id))
}

/// Get context window for a model, or the default.
pub fn context_window_for_model(model_id: &str) -> usize {
    lookup(model_id)
        .map_or(DEFAULT_CONTEXT_WINDOW, |e| e.context_window)
}

/// Get models for a specific provider.
pub fn models_for_provider(provider_id: &str) -> Vec<&'static ModelEntry> {
    MODEL_CATALOG
        .iter()
        .filter(|e| e.provider == provider_id)
        .collect()
}

/// Get all unique provider IDs in the catalog.
pub fn all_providers() -> Vec<&'static str> {
    let mut providers: Vec<&str> = MODEL_CATALOG
        .iter()
        .map(|e| e.provider)
        .collect();
    providers.sort_unstable();
    providers.dedup();
    providers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_not_empty() {
        assert!(!MODEL_CATALOG.is_empty());
    }

    #[test]
    fn test_lookup_exact() {
        // gpt-4o exists in multiple providers (openai, azure); lookup returns the first match
        let entry = lookup("gpt-4o");
        assert!(entry.is_some());
        assert!(entry.unwrap().id == "gpt-4o");
    }

    #[test]
    fn test_lookup_anthropic() {
        let entry = lookup("claude-sonnet-4-6");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().provider, "anthropic");
    }

    #[test]
    fn test_lookup_with_date_suffix() {
        // "claude-sonnet-4-6-20250514" should match "claude-sonnet-4-6"
        let entry = lookup("claude-sonnet-4-6-20250514");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().id, "claude-sonnet-4-6");
    }

    #[test]
    fn test_lookup_with_provider_prefix() {
        let entry = lookup("zai-coding-plan/glm-5");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().id, "glm-5");
    }

    #[test]
    fn test_lookup_unknown_returns_none() {
        assert!(lookup("totally-unknown-model-xyz").is_none());
    }

    #[test]
    fn test_context_window_for_known_model() {
        let cw = context_window_for_model("gpt-4o");
        assert!(cw > 0);
        assert_ne!(cw, DEFAULT_CONTEXT_WINDOW);
    }

    #[test]
    fn test_context_window_for_unknown_model() {
        let cw = context_window_for_model("future-model-3000");
        assert_eq!(cw, DEFAULT_CONTEXT_WINDOW);
    }

    #[test]
    fn test_models_for_provider() {
        let anthropic = models_for_provider("anthropic");
        assert!(!anthropic.is_empty());
        assert!(anthropic.iter().all(|m| m.provider == "anthropic"));
    }

    #[test]
    fn test_all_providers() {
        let providers = all_providers();
        assert!(!providers.is_empty());
        assert!(providers.contains(&"openai"));
        assert!(providers.contains(&"anthropic"));
    }

    #[test]
    fn test_catalog_sorted_by_provider() {
        for window in MODEL_CATALOG.windows(2) {
            assert!(
                window[0].provider <= window[1].provider,
                "Catalog not sorted: {} ({}) should come before {} ({})",
                window[0].provider,
                window[0].id,
                window[1].provider,
                window[1].id,
            );
        }
    }

    #[test]
    fn test_no_duplicate_ids_within_provider() {
        let mut seen = std::collections::HashSet::new();
        for entry in MODEL_CATALOG {
            let key = (entry.provider, entry.id);
            assert!(
                seen.insert(key),
                "Duplicate model: {}/{}",
                entry.provider,
                entry.id,
            );
        }
    }

    #[test]
    fn test_custom_models_present() {
        // Zhipu
        assert!(lookup("glm-5.1").is_some());
        assert!(lookup("glm-5").is_some());
        // Copilot
        assert!(lookup("gpt-5.5-copilot").is_some());
        // Ollama
        assert!(lookup("llama3").is_some());
        // LiteRT-LM
        assert!(lookup("gemma-4-e2b-it").is_some());
    }

    #[test]
    fn test_kimi_models_present() {
        let kimi_cn = models_for_provider("kimi-cn");
        assert!(!kimi_cn.is_empty());
        let kimi_global = models_for_provider("kimi-global");
        assert!(!kimi_global.is_empty());
    }
}
'''

    return header + "\n".join(entries) + footer


def main():
    output_path = Path(sys.argv[2]) if len(sys.argv) > 2 and sys.argv[1] == "--output" else DEFAULT_OUTPUT

    data = fetch_json(URL)

    # Extract from LiteLLM
    litellm_models = extract_models(data)

    # Merge with custom models
    all_models = litellm_models + CUSTOM_MODELS

    # Dedup: prefer LiteLLM entry over custom if same id+provider
    seen = {}
    for m in all_models:
        key = (m["id"], m["provider"])
        if key not in seen:
            seen[key] = m

    models = list(seen.values())

    # Generate Rust
    rust_code = generate_rust(models)

    # Write
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(rust_code)
    print(f"\nGenerated {output_path}")
    print(f"  {len(models)} models across {len(set(m['provider'] for m in models))} providers")

    # Summary by provider
    by_provider = {}
    for m in models:
        by_provider.setdefault(m["provider"], []).append(m["id"])
    for prov in sorted(by_provider):
        ids = sorted(by_provider[prov])
        print(f"  {prov}: {', '.join(ids)}")


if __name__ == "__main__":
    main()
