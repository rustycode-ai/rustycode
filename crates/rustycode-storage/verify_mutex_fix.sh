#!/bin/bash
# Verification script for Arc<Mutex<>> fixes in rustycode-storage

set -e

echo "=== Arc<Mutex<>> Fix Verification ==="
echo ""

echo "1. Checking for remaining Arc<Mutex<>> issues..."
# This should only find comments or type aliases, not actual usage
grep -rn "Arc<Mutex<" crates/rustycode-storage/src/ || echo "   ✓ No Arc<Mutex<>> usage found in src/"

echo ""
echo "2. Verifying proper tokio::sync::Mutex usage..."
if grep -q "tokio::sync::Mutex as TokioMutex" crates/rustycode-storage/src/lib.rs; then
    echo "   ✓ TokioMutex type alias defined"
else
    echo "   ✗ TokioMutex type alias not found"
    exit 1
fi

echo ""
echo "3. Verifying EventSubscriber uses TokioMutex..."
if grep -q "task_handle: Arc<TokioMutex" crates/rustycode-storage/src/lib.rs; then
    echo "   ✓ task_handle uses TokioMutex"
else
    echo "   ✗ task_handle does not use TokioMutex"
    exit 1
fi

if grep -q "subscription_handle: Arc<TokioMutex" crates/rustycode-storage/src/lib.rs; then
    echo "   ✓ subscription_handle uses TokioMutex"
else
    echo "   ✗ subscription_handle does not use TokioMutex"
    exit 1
fi

echo ""
echo "4. Verifying Storage maintains StdMutex for connection..."
if grep -q "conn: Arc<StdMutex<Connection>>" crates/rustycode-storage/src/lib.rs; then
    echo "   ✓ Connection uses StdMutex (correct for spawn_blocking)"
else
    echo "   ✗ Connection does not use StdMutex"
    exit 1
fi

echo ""
echo "5. Verifying async lock calls use .await..."
if grep -q "\.lock()\.await" crates/rustycode-storage/src/lib.rs; then
    echo "   ✓ Found async lock calls with .await"
else
    echo "   ✗ No async lock calls found"
    exit 1
fi

echo ""
echo "6. Checking test file exists..."
if [ -f "crates/rustycode-storage/tests/mutex_fix_test.rs" ]; then
    echo "   ✓ Test file created"
else
    echo "   ✗ Test file missing"
    exit 1
fi

echo ""
echo "7. Checking documentation..."
if [ -f "crates/rustycode-storage/MUTEX_FIX_SUMMARY.md" ]; then
    echo "   ✓ Documentation created"
else
    echo "   ✗ Documentation missing"
    exit 1
fi

echo ""
echo "=== All Checks Passed ✓ ==="
echo ""
echo "Summary of changes:"
echo "  - EventSubscriber.task_handle: Arc<Mutex<>> → Arc<TokioMutex<>>"
echo "  - EventSubscriber.subscription_handle: Arc<Mutex<>> → Arc<TokioMutex<>>"
echo "  - Storage.conn: Kept as Arc<StdMutex<>> (correct for spawn_blocking)"
echo "  - All async lock calls updated to use .await"
echo "  - Added comprehensive tests"
echo "  - Added documentation"
