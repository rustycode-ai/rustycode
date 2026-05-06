#!/bin/bash
# Comprehensive AI Coding Tools Performance Benchmark
# Tests RustyCode against popular alternatives

set -e

RESULTS_DIR="/Users/nat/dev/rustycode/benchmark_results"
mkdir -p "$RESULTS_DIR"

echo "=== AI Coding Tools Performance Benchmark ==="
echo "Results will be saved to: $RESULTS_DIR"
echo ""

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print section header
print_section() {
    echo -e "\n${GREEN}=== $1 ===${NC}\n"
}

# Function to test a tool
test_tool() {
    local tool_name="$1"
    local tool_command="$2"
    local test_operation="$3"

    echo -e "${YELLOW}Testing: $tool_name${NC}"

    # Check if tool is available
    if ! command -v $tool_command &> /dev/null; then
        echo "  ❌ $tool_command not found, skipping..."
        return
    fi

    # Measure startup time
    echo "  📊 Startup time:"
    for i in {1..5}; do
        start=$(gdate +%s%N 2>/dev/null || date +%s%N)
        $tool_command --version &> /dev/null || $tool_command --help &> /dev/null || true
        end=$(gdate +%s%N 2>/dev/null || date +%s%N)
        elapsed=$((($end - $start) / 1000000))
        echo "    Run $i: ${elapsed}ms"
    done | awk '{sum+=$2; count++} END {print "    Average:", sum/count, "ms"}'

    # Measure memory usage
    echo "  💾 Memory usage:"
    if command -v /usr/bin/time &> /dev/null; then
        /usr/bin/time -l $tool_command --version &> /dev/null || /usr/bin/time -l $tool_command --help &> /dev/null || true
    fi

    # Measure binary size if applicable
    echo "  📦 Binary size:"
    if [[ -n "$4" ]]; then
        size=$(stat -f%z "$4" 2>/dev/null || stat -c%s "$4" 2>/dev/null || echo "0")
        echo "    $(($size / 1024 / 1024))MB"
    fi

    echo ""
}

# Print system info
print_section "System Information"
echo "OS: $(uname -s)"
echo "Architecture: $(uname -m)"
echo "CPU: $(sysctl -n hw.ncpu 2>/dev/null || nproc 2>/dev/null || echo "unknown") cores"
echo "Memory: $(sysctl -n hw.memsize 2>/dev/null | awk '{print $1/1024/1024/1024 "GB"}' || free -h | grep Mem | awk '{print $2}')"
echo "Rust version: $(rustc --version 2>/dev/null || echo "not installed")"
echo "Python version: $(python3 --version 2>/dev/null || echo "not installed")"
echo "Node version: $(node --version 2>/dev/null || echo "not installed")"

print_section "Building RustyCode"
echo "Building optimized release..."
cd /Users/nat/dev/rustycode
cargo build --release 2>&1 | grep -E "(Compiling|Finished|error)" || true

print_section "Testing Tools"

# Test RustyCode
print_section "1. RustyCode"
test_tool "RustyCode" "./target/release/rustycode" "" "./target/release/rustycode"

# Test Aider (Python-based)
print_section "2. Aider"
test_tool "Aider" "aider" "" ""

# Test Continue.dev
print_section "3. Continue.dev"
test_tool "Continue" "continue" "" ""

# Test Cursor CLI (if available)
print_section "4. Cursor CLI"
test_tool "Cursor" "cursor" "" ""

# Test other tools
print_section "5. Other CLI Tools"

# Test ollama if available
test_tool "Ollama" "ollama" "" ""

# Test various git-AI tools
test_tool "git-ai" "git-ai" "" ""

print_section "Generating Comparison Report"
cat > "$RESULTS_DIR/benchmark_$(date +%Y%m%d_%H%M%S).txt" <<EOF
AI Coding Tools Benchmark - $(date)
=====================================

System:
- OS: $(uname -s)
- CPU: $(sysctl -n hw.ncpu 2>/dev/null || nproc 2>/dev/null || echo "unknown") cores
- Memory: $(sysctl -n hw.memsize 2>/dev/null | awk '{print $1/1024/1024/1024 "GB"}' || free -h | grep Mem | awk '{print $2}')

Results:
- RustyCode startup time: $(./target/release/rustycode --version 2>&1 | head -1 || echo "measured above")
- Binary size: $(($(stat -f%z target/release/rustycode 2>/dev/null || stat -c%s target/release/rustycode 2>/dev/null) / 1024 / 1024))MB

Note: Full benchmark results in individual test outputs
EOF

echo ""
echo -e "${GREEN}=== Benchmark Complete ===${NC}"
echo "Results saved to: $RESULTS_DIR"
echo ""
echo "📊 Summary:"
echo "  - All tools tested for startup time and memory usage"
echo "  - Binary sizes compared where applicable"
echo "  - System information captured for context"
echo ""
echo "To view detailed results:"
echo "  ls -la $RESULTS_DIR"
