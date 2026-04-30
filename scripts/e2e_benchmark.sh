#!/bin/bash
# RustyCode E2E Benchmark Suite
# Tests performance against common AI coding tool scenarios

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
RUSTYCODE_BIN="$PROJECT_DIR/target/release/rustycode-cli"
TEST_DIR="/tmp/rustycode_e2e_test_$(date +%s)"
RESULTS_FILE="/tmp/rustycode_e2e_results_$(date +%s).json"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper functions
log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[PASS]${NC} $1"; }
log_error() { echo -e "${RED}[FAIL]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }

# Measure execution time with millisecond precision (macOS compatible)
measure_time() {
    local start=$(python3 -c "import time; print(int(time.time() * 1000))")
    "$@" >/dev/null 2>&1
    local exit_code=$?
    local end=$(python3 -c "import time; print(int(time.time() * 1000))")
    local duration=$((end - start))
    printf '%d|%d\n' "$duration" "$exit_code"
}

# macOS-compatible timeout function
timeout_cmd() {
    local timeout_seconds=$1
    shift
    (
        eval "$@" &
        local pid=$!
        local elapsed=0
        while [ $elapsed -lt $((timeout_seconds * 1000)) ]; do
            if ! kill -0 $pid 2>/dev/null; then
                wait $pid
                return $?
            fi
            sleep 0.1
            elapsed=$((elapsed + 100))
        done
        kill $pid 2>/dev/null || true
        return 124  # timeout exit code
    )
}

# Create test directory
setup_test_env() {
    log_info "Setting up test environment in $TEST_DIR"
    mkdir -p "$TEST_DIR"
    cd "$TEST_DIR"

    # Initialize a git repo for realistic testing
    git init -q
    git config user.email "test@example.com"
    git config user.name "Test User"

    # Create a test project structure
    mkdir -p src tests

    # Create initial Cargo.toml
    cat > Cargo.toml << 'EOF'
[package]
name = "test-project"
version = "0.1.0"
edition = "2021"

[dependencies]
EOF
}

# Clean up test environment
cleanup_test_env() {
    log_info "Cleaning up test environment"
    cd /
    rm -rf "$TEST_DIR"
}

# Initialize results JSON
init_results() {
    cat > "$RESULTS_FILE" << EOF
{
  "test_run": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "rustycode_version": "0.1.0",
  "tests": []
}
EOF
}

# Add test result to JSON
add_result() {
    local test_name=$1
    local duration_ms=$2
    local status=$3
    local details=$4

    local temp_file="${RESULTS_FILE}.tmp"
    jq --arg name "$test_name" \
       --argjson duration_ms "$duration_ms" \
       --arg status "$status" \
       --arg details "$details" \
       '.tests += [{
         "name": $name,
         "duration_ms": ($duration_ms | tonumber),
         "status": $status,
         "details": $details
       }]' "$RESULTS_FILE" > "$temp_file"
    mv "$temp_file" "$RESULTS_FILE"
}

# ============================================================================
# TEST 1: Cold Start Performance
# ============================================================================
test_cold_start() {
    log_info "Test 1: Cold Start Performance"
    echo ""

    # Clear any file system cache
    sync && sudo purge 2>/dev/null || true

    local result
    result=$(measure_time "$RUSTYCODE_BIN" --version)
    local duration=${result%%|*}
    local exit_code=${result##*|}

    if [ $exit_code -eq 0 ]; then
        log_success "Cold start: ${duration}ms"
        add_result "cold_start" "$duration" "pass" "Version command executed successfully"
    else
        log_error "Cold start failed"
        add_result "cold_start" "$duration" "fail" "Exit code: $exit_code"
    fi
    echo ""
}

# ============================================================================
# TEST 2: Warm Start Performance
# ============================================================================
test_warm_start() {
    log_info "Test 2: Warm Start Performance"
    echo ""

    # Run a few times to warm up caches
    for i in {1..3}; do
        "$RUSTYCODE_BIN" --version >/dev/null 2>&1
    done

    local total=0
    for i in {1..5}; do
        local result
    result=$(measure_time "$RUSTYCODE_BIN" --version)
        local duration=${result%%|*}
        total=$((total + duration))
    done
    local avg=$((total / 5))

    log_success "Warm start (avg of 5): ${avg}ms"
    add_result "warm_start" "$avg" "pass" "Average of 5 consecutive runs"
    echo ""
}

# ============================================================================
# TEST 3: Simple Code Generation
# ============================================================================
test_simple_generation() {
    log_info "Test 3: Simple Code Generation"
    echo ""

    cat > src/simple.rs << 'EOF'
// TODO: Implement a function to calculate fibonacci numbers
EOF

    local prompt="Implement a fibonacci function in src/simple.rs that takes n: u32 and returns Option<u64>. Handle the base cases for n=0 and n=1."

    local result
    result=$(measure_time "$RUSTYCODE_BIN" run --auto --format json "$prompt" 2>&1)
    local duration=${result%%|*}
    local exit_code=${result##*|}

    if [ $exit_code -eq 0 ]; then
        # Check if file was modified
        if grep -q "fn fibonacci" src/simple.rs 2>/dev/null; then
            log_success "Simple generation: ${duration}ms - file modified"
            add_result "simple_generation" "$duration" "pass" "File modified with fibonacci implementation"
        else
            log_warn "Simple generation: ${duration}ms - but file not modified (tool execution may not be working)"
            add_result "simple_generation" "$duration" "partial" "LLM responded but file not modified (tool execution issue)"
        fi
    else
        log_error "Simple generation failed: $exit_code"
        add_result "simple_generation" "$duration" "fail" "Exit code: $exit_code"
    fi
    echo ""
}

# ============================================================================
# TEST 4: Code Refactoring Task
# ============================================================================
test_refactoring() {
    log_info "Test 4: Code Refactoring"
    echo ""

    cat > src/refactor.rs << 'EOF'
pub struct User {
    pub name: String,
    pub email: String,
    pub age: u32,
}

impl User {
    pub fn new(name: String, email: String, age: u32) -> Self {
        User { name, email, age }
    }

    pub fn is_adult(&self) -> bool {
        self.age >= 18
    }
}

pub fn process_users(users: Vec<User>) -> Vec<User> {
    let mut adults = Vec::new();
    for user in users {
        if user.is_adult() {
            adults.push(user);
        }
    }
    adults
}
EOF

    local prompt="Refactor src/refactor.rs to use builder pattern for User struct and add validation for email format."

    local result
    result=$(measure_time timeout_cmd 120 "$RUSTYCODE_BIN" run --auto --format json "$prompt" 2>&1)
    local duration=${result%%|*}
    local exit_code=${result##*|}

    # Check if builder pattern was added
    if grep -q "UserBuilder\|builder" src/refactor.rs 2>/dev/null; then
        log_success "Refactoring: ${duration}ms - builder pattern added"
        add_result "refactoring" "$duration" "pass" "Builder pattern implementation detected"
    elif [ $exit_code -eq 0 ]; then
        log_warn "Refactoring: ${duration}ms - completed but builder pattern not detected"
        add_result "refactoring" "$duration" "partial" "Completed but expected pattern not found"
    else
        log_error "Refactoring failed"
        add_result "refactoring" "$duration" "fail" "Command failed"
    fi
    echo ""
}

# ============================================================================
# TEST 5: Multi-file Task
# ============================================================================
test_multi_file() {
    log_info "Test 5: Multi-file Editing"
    echo ""

    mkdir -p src/models src/services

    cat > src/models/user.rs << 'EOF'
pub struct User {
    pub id: u32,
    pub name: String,
}
EOF

    cat > src/services/user_service.rs << 'EOF'
use crate::models::User;

pub struct UserService;

impl UserService {
    pub fn get_user(id: u32) -> Option<User> {
        None
    }
}
EOF

    local prompt="Add methods to UserService in src/services/user_service.rs: create_user, update_user, and delete_user. Update User struct in src/models/user.rs to include email field."

    local result
    result=$(measure_time timeout_cmd 120 "$RUSTYCODE_BIN" run --auto --format json "$prompt" 2>&1)
    local duration=${result%%|*}
    local exit_code=${result##*|}

    local files_modified=0
    grep -q "create_user\|update_user\|delete_user" src/services/user_service.rs 2>/dev/null && files_modified=$((files_modified + 1))
    grep -q "email" src/models/user.rs 2>/dev/null && files_modified=$((files_modified + 1))

    if [ $files_modified -eq 2 ]; then
        log_success "Multi-file: ${duration}ms - both files modified"
        add_result "multi_file" "$duration" "pass" "All expected files were modified"
    elif [ $exit_code -eq 0 ]; then
        log_warn "Multi-file: ${duration}ms - $files_modified/2 files modified"
        add_result "multi_file" "$duration" "partial" "$files_modified/2 files modified"
    else
        log_error "Multi-file failed"
        add_result "multi_file" "$duration" "fail" "Command failed"
    fi
    echo ""
}

# ============================================================================
# TEST 6: Error Explanation
# ============================================================================
test_error_explanation() {
    log_info "Test 6: Error Explanation"
    echo ""

    cat > src/error.rs << 'EOF'
fn main() {
    let v: Vec<i32> = vec![1, 2, 3];
    let fifth = v[4];
    println!("{}", fifth);
}
EOF

    # Compile to get error
    cargo build 2>/dev/null || true

    local prompt="Explain the error in src/error.rs and fix it using proper error handling."

    local result
    result=$(measure_time timeout_cmd 120 "$RUSTYCODE_BIN" run --auto --format json "$prompt" 2>&1)
    local duration=${result%%|*}
    local exit_code=${result##*|}

    if grep -q "get\|unwrap_or\|Option" src/error.rs 2>/dev/null; then
        log_success "Error explanation: ${duration}ms - proper error handling added"
        add_result "error_explanation" "$duration" "pass" "Error handling detected"
    elif [ $exit_code -eq 0 ]; then
        log_warn "Error explanation: ${duration}ms - file modified but expected pattern not found"
        add_result "error_explanation" "$duration" "partial" "File modified but pattern not detected"
    else
        log_error "Error explanation failed"
        add_result "error_explanation" "$duration" "fail" "Command failed"
    fi
    echo ""
}

# ============================================================================
# TEST 7: Test Generation
# ============================================================================
test_generation() {
    log_info "Test 7: Test Generation"
    echo ""

    cat > src/math.rs << 'EOF'
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn multiply(a: i32, b: i32) -> i32 {
    a * b
}
EOF

    local prompt="Write comprehensive unit tests for the functions in src/math.rs."

    local result
    result=$(measure_time timeout_cmd 120 "$RUSTYCODE_BIN" run --auto --mode test --format json "$prompt" 2>&1)
    local duration=${result%%|*}
    local exit_code=${result##*|}

    # Check if tests were created
    local tests_created=0
    [ -f tests/math_test.rs ] && tests_created=1
    grep -q "#\[test\]" src/math.rs 2>/dev/null && tests_created=1
    grep -q "#\[cfg(test)\]" src/math.rs 2>/dev/null && tests_created=1

    if [ $tests_created -eq 1 ]; then
        log_success "Test generation: ${duration}ms - tests created"
        add_result "test_generation" "$duration" "pass" "Test code generated"
    elif [ $exit_code -eq 0 ]; then
        log_warn "Test generation: ${duration}ms - completed but no tests detected"
        add_result "test_generation" "$duration" "partial" "Completed but tests not found in expected locations"
    else
        log_error "Test generation failed"
        add_result "test_generation" "$duration" "fail" "Command failed"
    fi
    echo ""
}

# ============================================================================
# TEST 8: Memory Usage
# ============================================================================
test_memory_usage() {
    log_info "Test 8: Memory Usage During Operation"
    echo ""

    # Measure memory before, during, and after operation
    local mem_before=$(ps -o rss= -p $$ | 2>/dev/null || echo "0")

    "$RUSTYCODE_BIN" run --auto "Write hello world function" >/dev/null 2>&1 &
    local pid=$!

    # Measure peak memory during operation
    local mem_peak=0
    for i in {1..30}; do
        sleep 0.1
        if ps -p $pid >/dev/null 2>&1; then
            local mem=$(ps -o rss= -p $pid 2>/dev/null || echo "0")
            mem=$((mem > mem_peak ? mem : mem_peak))
        else
            break
        fi
    done

    wait $pid 2>/dev/null || true

    # Convert KB to MB
    local mem_mb=$((mem_peak / 1024))

    log_success "Memory usage: peak ~${mem_mb}MB"
    add_result "memory_usage" "$mem_peak" "pass" "Peak RSS: ${mem_peak}KB (${mem_mb}MB)"
    echo ""
}

# ============================================================================
# TEST 9: Binary Size
# ============================================================================
test_binary_size() {
    log_info "Test 9: Binary Size"
    echo ""

    local size=$(wc -c < "$RUSTYCODE_BIN")
    local size_mb=$(echo "scale=2; $size / 1024 / 1024" | bc)

    log_success "Binary size: ${size_mb}MB (${size} bytes)"
    add_result "binary_size" "$size" "pass" "${size_mb}MB"
    echo ""
}

# ============================================================================
# Summary
# ============================================================================
print_summary() {
    log_info "Test Summary"
    echo ""

    local total=$(jq '.tests | length' "$RESULTS_FILE")
    local passed=$(jq '[.tests[].status] | map(select(. == "pass")) | length' "$RESULTS_FILE")
    local partial=$(jq '[.tests[].status] | map(select(. == "partial")) | length' "$RESULTS_FILE")
    local failed=$(jq '[.tests[].status] | map(select(. == "fail")) | length' "$RESULTS_FILE")

    echo "Total tests: $total"
    echo -e "${GREEN}Passed: $passed${NC}"
    echo -e "${YELLOW}Partial: $partial${NC}"
    echo -e "${RED}Failed: $failed${NC}"
    echo ""

    echo "Detailed Results:"
    jq -r '.tests[] | "\(.name): \(.duration_ms)ms - \(.status)"' "$RESULTS_FILE" | while read -r line; do
        local status=$(echo "$line" | cut -d' ' -f3)
        if [ "$status" = "pass" ]; then
            echo -e "${GREEN}$line${NC}"
        elif [ "$status" = "partial" ]; then
            echo -e "${YELLOW}$line${NC}"
        else
            echo -e "${RED}$line${NC}"
        fi
    done
    echo ""

    echo "Full results saved to: $RESULTS_FILE"
    echo ""
}

# ============================================================================
# Main
# ============================================================================
main() {
    echo "╔════════════════════════════════════════════════════════════════╗"
    echo "║     RustyCode E2E Benchmark Suite                             ║"
    echo "╚════════════════════════════════════════════════════════════════╝"
    echo ""

    # Check if binary exists
    if [ ! -f "$RUSTYCODE_BIN" ]; then
        log_error "Binary not found at $RUSTYCODE_BIN"
        log_info "Run: cargo build --release --package rustycode-cli"
        exit 1
    fi

    # Check dependencies
    for cmd in jq bc grep; do
        if ! command -v "$cmd" >/dev/null 2>&1; then
            log_error "Required command not found: $cmd"
            exit 1
        fi
    done

    # Initialize
    init_results
    setup_test_env

    # Run tests
    test_cold_start
    test_warm_start
    test_binary_size
    test_simple_generation
    test_refactoring
    test_multi_file
    test_error_explanation
    test_generation
    test_memory_usage

    # Summary
    print_summary

    # Cleanup
    cleanup_test_env
}

# Run main
main "$@"
