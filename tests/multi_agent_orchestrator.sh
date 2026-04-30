#!/bin/bash
# Multi-Agent Orchestrator
# Spawns multiple AI agents in parallel to test different providers/tasks

set -e

# Configuration
SESSION_PREFIX="rustycode-agent"
MAX_CONCURRENT_AGENTS=5
LOG_DIR="./agent_logs"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# Agent definitions
declare -A AGENTS
AGENTS[claude]="anthropic"
AGENTS[gpt]="openai"
AGENTS[gemini]="google"
AGENTS[local]="ollama"

# Task definitions
declare -a TASKS=(
    "ls:List files in current directory"
    "pwd:Show current directory"
    "grep:Find all TODO comments"
    "read:Read Cargo.toml"
    "math:What is 234 * 567?"
)

# Create log directory
mkdir -p "$LOG_DIR"

# ============================================
# Helper Functions
# ============================================

log() {
    local level="$1"
    shift
    local msg="$*"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    echo -e "${timestamp} [${level}] ${msg}"
}

log_info() {
    log "INFO" "${BLUE}$*${NC}"
}

log_success() {
    log "SUCCESS" "${GREEN}$*${NC}"
}

log_error() {
    log "ERROR" "${RED}$*${NC}"
}

log_warn() {
    log "WARN" "${YELLOW}$*${NC}"
}

print_header() {
    local title="$1"
    echo ""
    echo -e "${BOLD}${MAGENTA}╔════════════════════════════════════════════════════════════╗${NC}"
    printf "${BOLD}${MAGENTA}║  %-60s  ║\n" "$title"
    echo -e "${BOLD}${MAGENTA}╚════════════════════════════════════════════════════════════╝${NC}"
    echo ""
}

# ============================================
# Agent Management
# ============================================

spawn_agent() {
    local agent_name="$1"
    local provider="$2"
    local session_name="${SESSION_PREFIX}-${agent_name}-${TIMESTAMP}"
    local log_file="${LOG_DIR}/${agent_name}_${TIMESTAMP}.log"

    log_info "Spawning agent: $agent_name (provider: $provider)"

    # Create tmux session
    tmux new-session -d -s "$session_name" \
        "./target/release/rustycode-cli tui --provider $provider"

    # Wait for TUI to start
    sleep 2

    # Return session info
    echo "$session_name|$log_file"
}

kill_agent() {
    local session_name="$1"
    log_info "Killing agent session: $session_name"
    tmux kill-session -t "$session_name" 2>/dev/null || true
}

send_to_agent() {
    local session_name="$1"
    local prompt="$2"
    log_info "Sending to $session_name: $prompt"
    tmux send-keys -t "$session_name" "$prompt" Enter
}

capture_agent_output() {
    local session_name="$1"
    local wait_seconds="${2:-5}"
    sleep "$wait_seconds"
    tmux capture-pane -t "$session_name" -p
}

# ============================================
# Task Execution
# ============================================

execute_task_on_agent() {
    local agent_name="$1"
    local provider="$2"
    local task="$3"
    local session_info="$4"

    local session_name=$(echo "$session_info" | cut -d'|' -f1)
    local log_file=$(echo "$session_info" | cut -d'|' -f2)

    log_info "Executing task '$task' on agent $agent_name"

    # Send task to agent
    send_to_agent "$session_name" "$task"

    # Wait and capture output
    local output=$(capture_agent_output "$session_name" 8)

    # Save to log
    {
        echo "=== Task: $task ==="
        echo "=== Agent: $agent_name (provider: $provider) ==="
        echo "=== Time: $(date) ==="
        echo ""
        echo "$output"
        echo ""
        echo "=================================================="
        echo ""
    } >> "$log_file"

    # Extract result (look for tool output or AI response)
    local result=$(echo "$output" | grep -A 20 "tool\|assistant" | head -30)

    echo "$result"
}

# ============================================
# Parallel Execution
# ============================================

execute_parallel_tasks() {
    local tasks=("$@")
    local pids=()

    log_info "Starting ${#tasks[@]} parallel agent tasks..."

    # Execute tasks in background
    for i in "${!tasks[@]}"; do
        local task="${tasks[$i]}"
        local agent_name="agent-$i"
        local provider="${AGENTS[$agent_name:-claude]}"

        (
            local session_info=$(spawn_agent "$agent_name" "$provider")
            execute_task_on_agent "$agent_name" "$provider" "$task" "$session_info"
            local session_name=$(echo "$session_info" | cut -d'|' -f1)
            kill_agent "$session_name"
        ) &

        pids+=($!)
    done

    # Wait for all tasks to complete
    local completed=0
    local total=${#pids[@]}

    for pid in "${pids[@]}"; do
        if wait $pid; then
            ((completed++))
            log_success "Task completed ($completed/$total)"
        else
            log_error "Task failed ($completed/$total)"
        fi
    done

    log_info "All parallel tasks completed: $completed/$total"
}

# ============================================
# Comparison Mode
# ============================================

compare_agents_on_task() {
    local task="$1"
    shift
    local agents_to_test=("$@")

    print_header "Agent Comparison: $task"

    declare -A results
    declare -A pids
    declare -A sessions

    # Spawn all agents
    for agent in "${agents_to_test[@]}"; do
        local provider="${AGENTS[$agent]}"
        local session_info=$(spawn_agent "$agent" "$provider")
        sessions[$agent]="$session_info"
        log_info "Spawned $agent for comparison"
    done

    # Execute task on all agents
    for agent in "${agents_to_test[@]}"; do
        local provider="${AGENTS[$agent]}"
        local session_info="${sessions[$agent]}"

        (
            local result=$(execute_task_on_agent "$agent" "$provider" "$task" "$session_info")
            echo "$agent|$result" > "${LOG_DIR}/${agent}_result_${TIMESTAMP}.txt"
        ) &
        pids[$agent]=$!
    done

    # Wait for all results
    for agent in "${agents_to_test[@]}"; do
        wait ${pids[$agent]}
        log_success "$agent completed"
    done

    # Kill all sessions
    for agent in "${agents_to_test[@]}"; do
        local session_info="${sessions[$agent]}"
        local session_name=$(echo "$session_info" | cut -d'|' -f1)
        kill_agent "$session_name"
    done

    # Display comparison
    echo ""
    log_info "Comparison Results:"
    echo ""

    for agent in "${agents_to_test[@]}"; do
        local result_file="${LOG_DIR}/${agent}_result_${TIMESTAMP}.txt"
        if [[ -f "$result_file" ]]; then
            echo -e "${CYAN}═══ $agent ═══${NC}"
            cat "$result_file" | cut -d'|' -f2-
            echo ""
        fi
    done
}

# ============================================
# Coordination Mode
# ============================================

coordinate_multi_agent_workflow() {
    local workflow="$1"

    case "$workflow" in
        "code_review")
            print_header "Multi-Agent Code Review"

            local agent1="claude"
            local agent2="gpt"

            # Spawn agents
            local session1=$(spawn_agent "$agent1" "${AGENTS[$agent1]}")
            local session2=$(spawn_agent "$agent2" "${AGENTS[$agent2]}")

            # Agent 1: Find potential issues
            log_info "Agent 1 ($agent1): Analyzing code for issues..."
            send_to_agent "$(echo $session1 | cut -d'|' -f1)" "Find all potential bugs in src/"

            sleep 5

            # Agent 2: Suggest improvements
            log_info "Agent 2 ($agent2): Suggesting improvements..."
            send_to_agent "$(echo $session2 | cut -d'|' -f1)" "Review the code structure and suggest refactoring"

            sleep 5

            # Collect results
            local result1=$(capture_agent_output "$(echo $session1 | cut -d'|' -f1)" 3)
            local result2=$(capture_agent_output "$(echo $session2 | cut -d'|' -f1)" 3)

            # Cleanup
            kill_agent "$(echo $session1 | cut -d'|' -f1)"
            kill_agent "$(echo $session2 | cut -d'|' -f1)"

            # Display combined results
            echo ""
            log_info "Combined Analysis:"
            echo ""
            echo -e "${CYAN}═══ $agent1 Analysis ═══${NC}"
            echo "$result1" | head -40
            echo ""
            echo -e "${CYAN}═══ $agent2 Analysis ═══${NC}"
            echo "$result2" | head -40
            ;;

        "parallel_search")
            print_header "Parallel Multi-Agent Search"

            local search_terms=("TODO" "FIXME" "HACK" "XXX")
            declare -a pids

            for term in "${search_terms[@]}"; do
                (
                    local session=$(spawn_agent "search-$term" "${AGENTS[claude]}")
                    send_to_agent "$(echo $session | cut -d'|' -f1)" "grep -r \"$term\" src/ --count"
                    sleep 5
                    local result=$(capture_agent_output "$(echo $session | cut -d'|' -f1)" 3)
                    echo "$term: $result" > "${LOG_DIR}/search_${term}_${TIMESTAMP}.txt"
                    kill_agent "$(echo $session | cut -d'|' -f1)"
                ) &
                pids+=($!)
            done

            # Wait for all searches
            for pid in "${pids[@]}"; do
                wait $pid
            done

            # Combine results
            echo ""
            log_info "Combined Search Results:"
            echo ""
            for term in "${search_terms[@]}"; do
                echo -e "${CYAN}$term:${NC}"
                cat "${LOG_DIR}/search_${term}_${TIMESTAMP}.txt"
                echo ""
            done
            ;;

        *)
            log_error "Unknown workflow: $workflow"
            return 1
            ;;
    esac
}

# ============================================
# Interactive Mode
# ============================================

interactive_multi_agent() {
    print_header "Interactive Multi-Agent Mode"

    echo "Available agents:"
    for agent in "${!AGENTS[@]}"; do
        echo "  - $agent (provider: ${AGENTS[$agent]})"
    done
    echo ""

    echo "Select agents to spawn (comma-separated, or 'all'):"
    read -r agent_selection

    echo "Enter task to execute on all agents:"
    read -r task

    declare -a selected_agents

    if [[ "$agent_selection" == "all" ]]; then
        selected_agents=("${!AGENTS[@]}")
    else
        IFS=',' read -ra selected_agents <<< "$agent_selection"
    fi

    compare_agents_on_task "$task" "${selected_agents[@]}"
}

# ============================================
# Main
# ============================================

main() {
    print_header "Multi-Agent Orchestrator"

    # Parse command line args
    local mode="${1:-interactive}"
    shift || true

    case "$mode" in
        "parallel")
            local tasks=("$@")
            if [[ ${#tasks[@]} -eq 0 ]]; then
                # Use default tasks
                tasks=("${TASKS[@]}")
            fi
            execute_parallel_tasks "${tasks[@]}"
            ;;

        "compare")
            if [[ $# -lt 2 ]]; then
                log_error "Usage: $0 compare <task> <agent1> <agent2> ..."
                exit 1
            fi
            local task="$1"
            shift
            compare_agents_on_task "$task" "$@"
            ;;

        "workflow")
            local workflow="$1"
            coordinate_multi_agent_workflow "$workflow"
            ;;

        "interactive")
            interactive_multi_agent
            ;;

        "stress")
            print_header "Stress Test: Multiple Parallel Agents"
            local num_agents="${1:-5}"
            log_info "Spawning $num_agents parallel agents..."

            declare -a pids
            for i in $(seq 1 "$num_agents"); do
                (
                    local session=$(spawn_agent "stress-$i" "${AGENTS[claude]}")
                    send_to_agent "$(echo $session | cut -d'|' -f1)" "Calculate fibonacci(40)"
                    sleep 10
                    local result=$(capture_agent_output "$(echo $session | cut -d'|' -f1)" 3)
                    echo "Agent $i result: $result" >> "${LOG_DIR}/stress_${TIMESTAMP}.log"
                    kill_agent "$(echo $session | cut -d'|' -f1)"
                ) &
                pids+=($!)
            done

            # Wait for all
            for pid in "${pids[@]}"; do
                wait $pid
            done

            log_success "Stress test completed"
            ;;

        *)
            echo "Usage: $0 {parallel|compare|workflow|interactive|stress} [args...]"
            echo ""
            echo "Modes:"
            echo "  parallel     Execute tasks in parallel on different agents"
            echo "  compare      Compare agents on the same task"
            echo "  workflow     Run coordinated multi-agent workflows"
            echo "  interactive  Interactive multi-agent testing"
            echo "  stress       Stress test with many parallel agents"
            echo ""
            echo "Examples:"
            echo "  $0 parallel 'ls' 'pwd' 'whoami'"
            echo "  $0 compare 'What is 2+2?' claude gpt gemini"
            echo "  $0 workflow code_review"
            echo "  $0 interactive"
            echo "  $0 stress 10"
            exit 1
            ;;
    esac
}

# Cleanup on exit
cleanup() {
    log_info "Cleaning up agent sessions..."
    tmux list-sessions 2>/dev/null | grep "$SESSION_PREFIX" | cut -d: -f1 | \
        while read -r session; do
            tmux kill-session -t "$session" 2>/dev/null || true
        done
}

trap cleanup EXIT
trap cleanup INT TERM

main "$@"
