# rustycode-classification

Local task complexity classifier for orchestration routing.

## Purpose

Classify incoming tasks as "mundane" or "complex" to route:
- **Mundane**: Direct execution (fast path, no thinking overhead)
- **Complex**: Full orchestration pipeline (decomposition, escalation, learning)

## Examples

```rust
let classifier = LocalTaskClassifier::new();
let result = classifier.classify("list files in /tmp");
assert_eq!(result.complexity, TaskComplexity::Mundane);

let result = classifier.classify("refactor this Rust module to use async/await");
assert_eq!(result.complexity, TaskComplexity::Complex);
```

## Historical Pattern Integration

The classifier can optionally consult a `PatternQuery` implementation to check
if similar tasks have historically been complex. Implement the `PatternQuery`
trait and pass it via `LocalTaskClassifier::new().with_failure_store(store)`.

```rust
use rustycode_classification::{LocalTaskClassifier, PatternQuery, StoredPattern};

struct MyPatternStore;

impl PatternQuery for MyPatternStore {
    fn query_patterns(&self, task_type: &str) -> anyhow::Result<Vec<StoredPattern>> {
        // Query your historical data
        Ok(vec![])
    }
}

let store = std::sync::Arc::new(MyPatternStore);
let classifier = LocalTaskClassifier::new().with_failure_store(store);
```
