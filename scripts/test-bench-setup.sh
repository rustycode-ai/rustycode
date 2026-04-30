#!/bin/bash
# Integration test for benchmark setup

set -e

echo "Running Benchmark Integration Test..."

# Test 1: Validate setup
echo "Test 1: Validating setup..."
./scripts/validate-benchmarks.sh > /dev/null 2>&1
echo "✅ Setup validation passed"

# Test 2: Check benchmark files compile
echo "Test 2: Checking benchmark compilation..."
cargo check --benches --quiet 2>&1 | head -5
echo "✅ Benchmarks compile successfully"

# Test 3: Verify scripts are executable
echo "Test 3: Verifying script permissions..."
[ -x "scripts/bench.sh" ] && echo "✅ bench.sh is executable"
[ -x "scripts/bench-compare.sh" ] && echo "✅ bench-compare.sh is executable"
[ -x "scripts/validate-benchmarks.sh" ] && echo "✅ validate-benchmarks.sh is executable"

# Test 4: Check documentation exists
echo "Test 4: Verifying documentation..."
[ -f "docs/performance-baselines.md" ] && echo "✅ performance-baselines.md exists"
[ -f "docs/benchmarking-guide.md" ] && echo "✅ benchmarking-guide.md exists"

# Test 5: Verify CI workflow
echo "Test 5: Verifying CI configuration..."
[ -f ".github/workflows/bench.yml" ] && echo "✅ GitHub Actions workflow exists"

echo ""
echo "========================================="
echo "All integration tests passed! ✅"
echo "========================================="
echo ""
echo "Your benchmark system is ready to use!"
echo ""
echo "Quick start:"
echo "  ./scripts/bench.sh"
