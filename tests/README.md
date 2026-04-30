# RustyCode Integration Test Suite

Comprehensive integration tests for the RustyCode AI assistant system.

## Test Structure

```
tests/
├── integration_new/          # Integration tests
│   ├── config_integration.rs      # Configuration system tests
│   ├── provider_integration.rs    # Provider registry tests
│   ├── session_integration.rs     # Session management tests
│   ├── mcp_integration.rs         # MCP protocol tests
│   └── e2e_workflow.rs            # End-to-end workflow tests
├── property/                 # Property-based tests
│   ├── config_properties.rs       # Config invariants
│   └── session_properties.rs      # Session invariants
├── common/                   # Test utilities
│   └── mod.rs                     # Helpers, fixtures, env management
├── fixtures/                 # Test data
│   ├── configs/                   # Config fixtures
│   ├── sessions/                  # Session fixtures
│   └── mcp/                       # MCP fixtures
└── run_integration_tests.sh  # Test runner script
```

## Test Categories

### 1. Configuration Integration Tests (`config_integration.rs`)

Tests for the hierarchical configuration system:

- **Hierarchical Loading**: Merging configs from global → workspace → project
- **Environment Substitution**: `{env:VAR}` expansion
- **File Reference Substitution**: `{file:path}` expansion
- **Provider Discovery**: Auto-detection from environment variables
- **Validation**: Config schema validation
- **JSONC Support**: Comments and trailing commas
- **Default Values**: Ensuring sensible defaults

**Count**: 10 tests

### 2. Provider Integration Tests (`provider_integration.rs`)

Tests for the LLM provider registry:

- **Provider Bootstrap**: Auto-discovery from environment
- **Model Registry**: Listing and querying available models
- **Cost Tracking**: Accurate cost calculation and accumulation
- **Multi-Provider**: Using multiple providers in one session
- **Provider Capabilities**: Streaming, function calling, vision
- **Currency Handling**: Pricing in different currencies
- **Cost Reset**: Clearing cost accumulators

**Count**: 13 tests

### 3. Session Integration Tests (`session_integration.rs`)

Tests for session and message management:

- **Session Lifecycle**: Create, use, fork, archive, delete
- **Message Compaction**: Various compaction strategies
- **Serialization**: Save/load with compression
- **Token Accounting**: Accurate token counting
- **Metadata**: Session metadata and tags
- **Message Types**: Text, code blocks, tool use/results
- **Forking**: Creating independent session copies
- **Clear Operations**: Removing all messages

**Count**: 15 tests

### 4. MCP Integration Tests (`mcp_integration.rs`)

Tests for Model Context Protocol integration:

- **Server Lifecycle**: Start, use, stop MCP servers
- **Tool Discovery**: Finding available tools
- **Tool Calling**: Invoking MCP tools
- **Resource Access**: Reading MCP resources
- **Concurrent Requests**: Parallel MCP operations
- **Multiple Servers**: Managing multiple connections
- **Error Handling**: Graceful failure handling
- **Prompt Templates**: MCP prompt management

**Count**: 12 tests

### 5. End-to-End Workflow Tests (`e2e_workflow.rs`)

Complete workflow simulations:

- **Coding Workflow**: Full development task with agents
- **Debugging Workflow**: Error analysis and recovery
- **Refactoring Workflow**: Multi-step code improvement
- **Tool Calling**: Integration with tools (Read, Bash, etc.)
- **Session Persistence**: Save/load across workflows
- **Multi-Agent**: Multiple agents collaborating
- **Error Recovery**: Handling and recovering from errors
- **Iterative Refinement**: Improving solutions over time

**Count**: 8 tests

### 6. Property-Based Tests (`property/`)

Invariant testing with proptest:

- **Config Roundtrip**: Serialize/deserialize preserves data
- **Token Counting**: Monotonic increase with messages
- **Compaction**: Always reduces message/token count
- **Immutability**: Clones don't affect originals
- **Session IDs**: Unique generation
- **Message Order**: Preserved during operations

**Count**: 25+ properties

## Running Tests

### Run All Tests

```bash
./tests/run_integration_tests.sh
```

### Run Specific Category

```bash
# Configuration tests
cargo test --test config_integration

# Provider tests
cargo test --test provider_integration

# Session tests
cargo test --test session_integration

# MCP tests
cargo test --test mcp_integration

# E2E workflow tests
cargo test --test e2e_workflow
```

### Run Property-Based Tests

```bash
# Config properties
cargo test --test config_properties

# Session properties
cargo test --test session_properties
```

### Run with Output

```bash
cargo test --test config_integration -- --nocapture
```

### Run Single Test

```bash
cargo test --test config_integration test_hierarchical_config_loading -- --nocapture
```

## Test Fixtures

### Configuration Fixtures

Located in `tests/fixtures/configs/`:

- `basic_config.json` - Simple configuration
- `provider_config.json` - Multi-provider setup
- `workspace_config.json` - Workspace configuration
- `substitution_config.json` - Environment/file substitutions

### Session Fixtures

Located in `tests/fixtures/sessions/`:

- `simple_session.json` - Basic conversation
- `complex_session.json` - Multiple message types, code blocks

### MCP Fixtures

Located in `tests/fixtures/mcp/`:

- `test_server_config.json` - Sample MCP server configuration

## Test Helpers

### TestConfig

Creates temporary directories for testing:

```rust
let config = TestConfig::new();
config.write_config("config.json", r#"{"model": "test"}"#);
```

### TestEnv

Manages environment variables with automatic cleanup:

```rust
let mut env = TestEnv::new();
env.set("API_KEY", "test-key");
env.remove("OTHER_VAR");
// Automatically restored on drop
```

### retry_async

Retries async operations with backoff:

```rust
let result = retry_async(
    || async { operation().await },
    max_attempts,
    delay,
).await?;
```

## Coverage

### Test Coverage by Crate

| Crate | Unit Tests | Integration Tests | Property Tests | Coverage |
|-------|-----------|-------------------|----------------|----------|
| rustycode-config | ✓ | 10 tests | 9 properties | >80% |
| rustycode-providers | ✓ | 13 tests | - | >75% |
| rustycode-session | ✓ | 15 tests | 16 properties | >80% |
| rustycode-mcp | ✓ | 12 tests | - | >70% |
| rustycode-core | ✓ | 8 tests (E2E) | - | >70% |
| **Total** | **50+** | **58 tests** | **25 properties** | **>75%** |

## Success Criteria

✅ **20+ integration tests** covering all major systems
✅ **End-to-end workflow tests** simulating real usage
✅ **Property-based tests** verifying invariants
✅ **Test fixtures and helpers** for easy testing
✅ **Comprehensive coverage** (>70% overall)

## Contributing

When adding new features:

1. **Add unit tests** in the crate's `tests/` directory
2. **Add integration tests** in the appropriate category
3. **Add property tests** for invariants
4. **Update fixtures** if needed
5. **Run full suite** before submitting

## Troubleshooting

### Tests Failing with "Environment Variable Not Found"

Ensure required environment variables are set or use `TestEnv` to set them:

```rust
let mut env = TestEnv::new();
env.set("ANTHROPIC_API_KEY", "test-key");
```

### MCP Tests Failing

MCP tests require actual MCP server binaries. They're designed to fail gracefully if servers aren't available.

### Temporary Files Not Cleaned Up

`TestConfig` uses `tempfile` which automatically cleans up on drop. If tests crash, check `/tmp` for orphaned files.

## CI/CD Integration

These tests are designed to run in CI/CD pipelines:

```yaml
- name: Run integration tests
  run: ./tests/run_integration_tests.sh
  env:
    ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
    OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}
```

## License

MIT License - See LICENSE file for details.
