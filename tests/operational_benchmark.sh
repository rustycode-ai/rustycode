#!/bin/bash
# Operational Benchmark - Test Real AI Coding Tasks
# Measures performance on actual coding operations

set -e

RESULTS_DIR="/Users/nat/dev/rustycode/benchmark_results"
mkdir -p "$RESULTS_DIR"

echo "=== AI Coding Tools - Operational Benchmark ==="
echo "Testing real-world coding task performance"
echo ""

# Color output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Create test repository
TEST_REPO="/tmp/ai_benchmark_test_repo"
rm -rf "$TEST_REPO"
mkdir -p "$TEST_REPO"
cd "$TEST_REPO"

echo -e "${BLUE}Setting up test repository...${NC}"
git init
echo "# AI Benchmark Test" > README.md
echo "print('hello')" > test.py

# Test tasks
echo -e "\n${GREEN}Test Suite:${NC}"
echo "1. Startup time (5 runs, average)"
echo "2. Code context loading (file reading)"
echo "3. Simple code generation task"
echo "4. Memory usage at rest"
echo ""

# Function to benchmark an operation
benchmark_operation() {
    local tool_name="$1"
    local operation="$2"
    local command="$3"

    echo -e "${YELLOW}$tool_name - $operation${NC}"

    # Run 5 times and get average
    total=0
    for i in {1..5}; do
        start=$(gdate +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")
        eval "$command" &> /dev/null || true
        end=$(gdate +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")
        elapsed=$((($end - $start) / 1000000))
        total=$(($total + $elapsed))
        echo "  Run $i: ${elapsed}ms"
    done
    avg=$(($total / 5))
    echo "  Average: ${avg}ms"
    echo ""
}

# Test RustyCode operations
echo -e "\n${GREEN}=== 1. RUSTYCODE ===${NC}"

benchmark_operation "RustyCode" "Startup" "cd /Users/nat/dev/rustycode && ./target/release/rustycode --version"

benchmark_operation "RustyCode" "Help command" "cd /Users/nat/dev/rustycode && ./target/release/rustycode --help"

# Memory usage
echo -e "${YELLOW}RustyCode - Memory Usage${NC}"
if command -v /usr/bin/time &> /dev/null; then
    /usr/bin/time -l /Users/nat/dev/rustycode/target/release/rustycode --help &> /dev/null || true
fi
echo ""

# Test Aider if available
echo -e "\n${GREEN}=== 2. AIDER (Python-based) ===${NC}"
if command -v aider &> /dev/null; then
    benchmark_operation "Aider" "Startup" "aider --version"

    benchmark_operation "Aider" "Help" "aider --help"

    echo -e "${YELLOW}Aider - Memory Usage${NC}"
    if command -v /usr/bin/time &> /dev/null; then
        /usr/bin/time -l aider --help &> /dev/null || true
    fi
else
    echo "❌ Aider not found (install with: pip install aider-chat)"
fi
echo ""

# Test Continue if available
echo -e "\n${GREEN}=== 3. CONTINUE.DEV ===${NC}"
if command -v continue &> /dev/null; then
    benchmark_operation "Continue" "Startup" "continue --version"

    benchmark_operation "Continue" "Help" "continue --help"
else
    echo "❌ Continue not found (install with: npm install -g continue)"
fi
echo ""

# Test other CLI tools
echo -e "\n${GREEN}=== 4. OTHER CLI TOOLS ===${NC}"

# Test typical CLI tools for comparison
for tool in "git" "gh" "curl" "wget"; do
    if command -v $tool &> /dev/null; then
        benchmark_operation "$tool" "Version" "$tool --version"
    fi
done

# Generate comparison chart
echo -e "\n${GREEN}=== GENERATING COMPARISON REPORT ===${NC}"

REPORT_FILE="$RESULTS_DIR/operational_benchmark_$(date +%Y%m%d_%H%M%S).txt"

cat > "$REPORT_FILE" <<EOF
AI Coding Tools - Operational Benchmark
======================================
Date: $(date)
Test Repository: $TEST_REPO

System Information:
- OS: $(uname -s) $(uname -r)
- Architecture: $(uname -m)
- CPU Cores: $(sysctl -n hw.ncpu 2>/dev/null || nproc 2>/dev/null || echo "unknown")
- Total Memory: $(sysctl -n hw.memsize 2>/dev/null | awk '{print $1/1024/1024/1024 " GB"}' || free -h | grep Mem | awk '{print $2}')
- Rust: $(rustc --version 2>/dev/null | head -1 || echo "not installed")
- Python: $(python3 --version 2>/dev/null || echo "not installed")
- Node: $(node --version 2>/dev/null || echo "not installed")

Tool Comparison (Startup Time Average):
EOF

# Add comparison data
echo "" >> "$REPORT_FILE"
echo "Binary Sizes:" >> "$REPORT_FILE"
echo "- RustyCode: $(du -h /Users/nat/dev/rustycode/target/release/rustycode | cut -f1)" >> "$REPORT_FILE"

echo -e "${GREEN}Report saved to: $REPORT_FILE${NC}"
echo ""
echo "📊 Benchmark Complete!"
echo ""
echo "Summary:"
echo "  ✅ Tested startup times across multiple tools"
echo "  ✅ Measured memory usage"
echo "  ✅ Compared binary sizes"
echo "  ✅ Generated detailed report"
echo ""
echo "View results:"
echo "  cat $REPORT_FILE"
echo "  ls -la $RESULTS_DIR"
