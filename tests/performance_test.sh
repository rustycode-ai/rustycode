#!/bin/bash
# Performance test script for rustycode

echo "=== RustyCode Performance Test ==="
echo "Testing build: $1"
echo ""

# Measure compile time
echo "1. Measuring compile time..."
start_time=$(date +%s%N)
cargo build --release 2>&1 | grep -E "(Compiling|Finished)"
end_time=$(date +%s%N)
compile_time=$((($end_time - $start_time) / 1000000))
echo "   Compile time: ${compile_time}ms"
echo ""

# Measure binary size
echo "2. Measuring binary size..."
binary_size=$(stat -f%z target/release/rustycode 2>/dev/null || stat -c%s target/release/rustycode 2>/dev/null || echo "0")
echo "   Binary size: $(($binary_size / 1024 / 1024))MB"
echo ""

# Test with hyperfine if available
if command -v hyperfine &> /dev/null; then
    echo "3. Running startup time benchmark (hyperfine)..."
    hyperfine --warmup 3 'target/release/rustycode --version' 2>&1 | grep -E "(Time|Mean)"
    echo ""
fi

# Memory usage with /usr/bin/time
if command -v /usr/bin/time &> /dev/null; then
    echo "4. Measuring memory usage..."
    /usr/bin/time -l cargo run --release -- --help 2>&1 | grep "maximum resident" || echo "   Memory: N/A"
    echo ""
fi

echo "=== Test Complete ==="
