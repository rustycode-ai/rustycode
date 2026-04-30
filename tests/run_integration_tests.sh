#!/bin/bash
# Comprehensive Integration Test Runner
# Runs all integration tests with proper setup and reporting

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test categories
CATEGORIES=("config" "provider" "session" "mcp" "e2e" "property")

# Counters
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

echo "=================================="
echo "RustyCode Integration Test Suite"
echo "=================================="
echo ""

# Function to run tests for a category
run_category_tests() {
    local category=$1
    local test_file=$2

    echo -e "${YELLOW}Running $category tests...${NC}"

    if [ ! -f "$test_file" ]; then
        echo -e "${RED}Test file not found: $test_file${NC}"
        return 1
    fi

    # Run tests and capture output
    if cargo test --test "$test_file" --nocapture 2>&1 | tee "/tmp/${category}_test.log"; then
        echo -e "${GREEN}✓ $category tests passed${NC}"
        return 0
    else
        echo -e "${RED}✗ $category tests failed${NC}"
        return 1
    fi
}

# Function to count tests in a file
count_tests() {
    local file=$1
    grep -c "#\[tokio::test\]\|#\[test\]" "$file" || echo "0"
}

# Main test execution
echo "Scanning for test files..."
echo ""

# Count total tests
for category in "${CATEGORIES[@]}"; do
    case $category in
        config)
            file="tests/integration_new/config_integration.rs"
            ;;
        provider)
            file="tests/integration_new/provider_integration.rs"
            ;;
        session)
            file="tests/integration_new/session_integration.rs"
            ;;
        mcp)
            file="tests/integration_new/mcp_integration.rs"
            ;;
        e2e)
            file="tests/integration_new/e2e_workflow.rs"
            ;;
        property)
            file="tests/property/config_properties.rs"
            ;;
    esac

    if [ -f "$file" ]; then
        count=$(count_tests "$file")
        TOTAL_TESTS=$((TOTAL_TESTS + count))
        echo "  $category: $count tests"
    fi
done

echo ""
echo "Total tests found: $TOTAL_TESTS"
echo ""
echo "=================================="
echo ""

# Run tests
for category in "${CATEGORIES[@]}"; do
    case $category in
        config)
            file="tests/integration_new/config_integration.rs"
            ;;
        provider)
            file="tests/integration_new/provider_integration.rs"
            ;;
        session)
            file="tests/integration_new/session_integration.rs"
            ;;
        mcp)
            file="tests/integration_new/mcp_integration.rs"
            ;;
        e2e)
            file="tests/integration_new/e2e_workflow.rs"
            ;;
        property)
            file="tests/property/config_properties.rs"
            ;;
    esac

    if run_category_tests "$category" "$file"; then
        count=$(count_tests "$file")
        PASSED_TESTS=$((PASSED_TESTS + count))
    else
        count=$(count_tests "$file")
        FAILED_TESTS=$((FAILED_TESTS + count))
    fi

    echo ""
done

# Property-based tests
echo -e "${YELLOW}Running additional property tests...${NC}"
if cargo test --test session_properties 2>&1 | tee "/tmp/property_session_test.log"; then
    echo -e "${GREEN}✓ Session property tests passed${NC}"
else
    echo -e "${RED}✗ Session property tests failed${NC}"
    FAILED_TESTS=$((FAILED_TESTS + 1))
fi

echo ""
echo "=================================="
echo "Test Results Summary"
echo "=================================="
echo -e "Total tests:  $TOTAL_TESTS"
echo -e "${GREEN}Passed:       $PASSED_TESTS${NC}"
echo -e "${RED}Failed:       $FAILED_TESTS${NC}"
echo ""

if [ $FAILED_TESTS -eq 0 ]; then
    echo -e "${GREEN}All tests passed! ✓${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed. Check logs in /tmp/*.log${NC}"
    exit 1
fi
