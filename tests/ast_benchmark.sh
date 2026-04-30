#!/bin/bash
# AST Benchmark Harness - Empirical Validation of AST Integration
# Runs a matrix of 13 tasks x 4 strategies to measure performance and effectiveness.

set -e

RESULTS_DIR="/Users/nat/dev/rustycode/benchmark_results/ast_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"
CSV_REPORT="$RESULTS_DIR/benchmark_matrix.csv"

echo "task_id,strategy,ast_enabled,duration_ms,status" > "$CSV_REPORT"

# Tasks list (IDs match documentation)
TASKS=("T01" "T02" "T03" "T04" "T05" "T06" "T07" "T08" "T09" "T10" "T11" "T12" "T13")
STRATEGIES=("direct" "sequential" "phased" "ast-standard")

echo "=== AST Benchmark Matrix Initiated ==="
echo "Report: $CSV_REPORT"

for task in "${TASKS[@]}"; do
    for strategy in "${STRATEGIES[@]}"; do
        echo -e "\nRunning Task: $task | Strategy: $strategy"
        
        USE_AST="false"
        if [ "$strategy" == "ast-standard" ]; then
            USE_AST="true"
        fi

        start=$(gdate +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")
        
        # Invoke CLI with appropriate flags
        # Assuming run_auto orchestration loop
        if [ "$USE_AST" == "true" ]; then
             cargo run -p rustycode-cli --bin rustycode-cli -- orchestra auto --use-ast --budget 10 &> "$RESULTS_DIR/${task}_${strategy}.log"
        else
             cargo run -p rustycode-cli --bin rustycode-cli -- orchestra auto --budget 10 &> "$RESULTS_DIR/${task}_${strategy}.log"
        fi
        
        exit_code=$?
        
        end=$(gdate +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")
        elapsed=$((($end - $start) / 1000000))
        
        status="success"
        if [ $exit_code -ne 0 ]; then
            status="failed"
        fi
        
        echo "$task,$strategy,$USE_AST,$elapsed,$status" >> "$CSV_REPORT"
        echo "  Status: $status | Duration: ${elapsed}ms"
    done
done

echo -e "\n=== Benchmark Complete ==="
echo "Results available in: $RESULTS_DIR"
