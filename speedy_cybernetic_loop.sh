#!/etc/profiles/per-user/logger/bin/bash
# speedy_cybernetic_loop.sh
# Cybernetic Control Loop for Speedy Audio Visualizer Development
# Based on the control system documented in docs/update-speedy-v1.0.md

set -euo pipefail

# Initialize log file
LOG_FILE="cybernetic_loop.log"
TIMESTAMP=$(date '+%Y-%m-%d %H:%M:%S')
LEARNING_DIR="learning"

echo "🚀 Starting Speedy Cybernetic Development Loop" | tee -a "$LOG_FILE"
echo "Timestamp: $TIMESTAMP" | tee -a "$LOG_FILE"
echo "========================================" | tee -a "$LOG_FILE"

# Create learning directory structure if it doesn't exist
mkdir -p "$LEARNING_DIR"
touch "$LEARNING_DIR/performance_patterns.md"
touch "$LEARNING_DIR/failure_recovery_log.md"
touch "$LEARNING_DIR/dependency_evolution.md"
touch "$LEARNING_DIR/user_feedback_analysis.md"
touch "$LEARNING_DIR/control_loop_tuning.md"

# Global variables for state tracking
BUILD_STATUS=""
AUDIO_TEST_STATUS=""
VISUAL_TEST_STATUS=""
PERFORMANCE_STATUS=""
LEARNING_ENTRIES=()

# Phase 1: SENSING 🔍
echo "🔍 Phase 1: SENSING" | tee -a "$LOG_FILE"
if [[ -x "scripts/sense_project_state.sh" ]]; then
    ./scripts/sense_project_state.sh | tee -a "$LOG_FILE"
else
    echo "⚠️  sense_project_state.sh not found, running basic sensing..." | tee -a "$LOG_FILE"
    
    # Basic code quality sensing
    echo "  • Checking compilation status..." | tee -a "$LOG_FILE"
    if nix develop --command cargo check --all-targets --all-features 2>/dev/null; then
        echo "    ✅ Compilation: PASS" | tee -a "$LOG_FILE"
        BUILD_STATUS="PASS"
    else
        echo "    ❌ Compilation: FAIL" | tee -a "$LOG_FILE"
        BUILD_STATUS="FAIL"
    fi
    
    # Basic audio system sensing
    echo "  • Checking audio system..." | tee -a "$LOG_FILE"
    if aplay -l &>/dev/null; then
        echo "    ✅ Audio devices available" | tee -a "$LOG_FILE"
        AUDIO_TEST_STATUS="PASS"
    else
        echo "    ⚠️  No audio devices detected" | tee -a "$LOG_FILE"
        AUDIO_TEST_STATUS="WARN"
    fi
    
    # Dependencies sensing
    echo "  • Checking dependencies..." | tee -a "$LOG_FILE"
    if nix develop --command cargo --version &>/dev/null; then
        echo "    ✅ Rust toolchain available" | tee -a "$LOG_FILE"
    else
        echo "    ❌ Rust toolchain unavailable" | tee -a "$LOG_FILE"
    fi
fi

# Phase 2: COMPARISON 📊
echo "📊 Phase 2: COMPARISON" | tee -a "$LOG_FILE"
if [[ -x "scripts/compare_against_targets.sh" ]]; then
    ./scripts/compare_against_targets.sh | tee -a "$LOG_FILE"
else
    echo "⚠️  compare_against_targets.sh not found, running basic comparison..." | tee -a "$LOG_FILE"
    
    # Quality targets comparison
    echo "  • Comparing against quality targets..." | tee -a "$LOG_FILE"
    if [[ "$BUILD_STATUS" = "PASS" ]]; then
        echo "    ✅ Compilation target: MET (100% success)" | tee -a "$LOG_FILE"
    else
        echo "    ❌ Compilation target: MISSED (compilation failures)" | tee -a "$LOG_FILE"
    fi
    
    if [[ "$AUDIO_TEST_STATUS" = "PASS" ]]; then
        echo "    ✅ Audio system target: MET (devices available)" | tee -a "$LOG_FILE"
    else
        echo "    ⚠️  Audio system target: PARTIAL (limited audio capability)" | tee -a "$LOG_FILE"
    fi
fi

# Phase 3: ACTION ⚡
echo "⚡ Phase 3: ACTION" | tee -a "$LOG_FILE"
if [[ -x "scripts/execute_improvements.sh" ]]; then
    ./scripts/execute_improvements.sh | tee -a "$LOG_FILE"
else
    echo "⚠️  execute_improvements.sh not found, running basic actions..." | tee -a "$LOG_FILE"
    
    # Basic code quality actions
    if [[ "$BUILD_STATUS" = "FAIL" ]]; then
        echo "  • Attempting to fix build issues..." | tee -a "$LOG_FILE"
        
        # Try formatting first
        if nix develop --command cargo fmt 2>/dev/null; then
            echo "    ✅ Code formatting applied" | tee -a "$LOG_FILE"
        else
            echo "    ⚠️  Code formatting failed" | tee -a "$LOG_FILE"
        fi
        
        # Re-test compilation
        if nix develop --command cargo check --all-targets 2>/dev/null; then
            echo "    ✅ Build issues resolved" | tee -a "$LOG_FILE"
            BUILD_STATUS="PASS"
            LEARNING_ENTRIES+=("Build fixed by cargo fmt")
        else
            echo "    ❌ Build issues persist" | tee -a "$LOG_FILE"
        fi
    else
        echo "  • No immediate actions required (build passing)" | tee -a "$LOG_FILE"
    fi
fi

# Phase 4: FEEDBACK 🔄
echo "🔄 Phase 4: FEEDBACK" | tee -a "$LOG_FILE"
if [[ -x "scripts/validate_improvements.sh" ]]; then
    ./scripts/validate_improvements.sh | tee -a "$LOG_FILE"
else
    echo "⚠️  validate_improvements.sh not found, running basic validation..." | tee -a "$LOG_FILE"
    
    # Re-validate build status after actions
    echo "  • Validating build status..." | tee -a "$LOG_FILE"
    if nix develop --command cargo build --all-targets 2>/dev/null; then
        echo "    ✅ BUILD VALIDATION: SUCCESS" | tee -a "$LOG_FILE"
        BUILD_STATUS="SUCCESS"
    else
        echo "    ❌ BUILD VALIDATION: FAILURE" | tee -a "$LOG_FILE"
        BUILD_STATUS="FAILURE"
        
        # Recovery attempt
        echo "  • Attempting recovery..." | tee -a "$LOG_FILE"
        if git log --oneline -1 &>/dev/null; then
            echo "    • Git repository detected, considering rollback..." | tee -a "$LOG_FILE"
            # Don't automatically rollback in this basic version
            LEARNING_ENTRIES+=("Build failure occurred - consider implementing automatic rollback")
        fi
    fi
    
    # Basic performance validation
    echo "  • Validating system resources..." | tee -a "$LOG_FILE"
    MEMORY_USAGE=$(free -m | awk 'NR==2{printf "%.1f", $3*100/$2}')
    echo "    • Memory usage: ${MEMORY_USAGE}%" | tee -a "$LOG_FILE"
    
    if (( $(echo "$MEMORY_USAGE < 80" | bc -l) )); then
        echo "    ✅ Memory usage within acceptable range" | tee -a "$LOG_FILE"
    else
        echo "    ⚠️  High memory usage detected" | tee -a "$LOG_FILE"
        LEARNING_ENTRIES+=("High memory usage: ${MEMORY_USAGE}% - investigate optimization opportunities")
    fi
fi

# Phase 5: LEARNING 📚
echo "📚 Phase 5: LEARNING" | tee -a "$LOG_FILE"
if [[ -x "scripts/capture_learning.sh" ]]; then
    ./scripts/capture_learning.sh | tee -a "$LOG_FILE"
else
    echo "⚠️  capture_learning.sh not found, running basic learning capture..." | tee -a "$LOG_FILE"
    
    # Record learning entries
    echo "  • Capturing learning insights..." | tee -a "$LOG_FILE"
    
    # Performance patterns
    {
        echo "## Performance Learning Entry - $TIMESTAMP"
        echo "- Build Status: $BUILD_STATUS"
        echo "- Audio System: $AUDIO_TEST_STATUS"  
        echo "- Memory Usage: ${MEMORY_USAGE}%"
        echo "- Loop Duration: $SECONDS seconds"
        echo ""
    } >> "$LEARNING_DIR/performance_patterns.md"
    
    # Learning insights
    if [[ ${#LEARNING_ENTRIES[@]} -gt 0 ]]; then
        {
            echo "## Learning Insights - $TIMESTAMP"
            for entry in "${LEARNING_ENTRIES[@]}"; do
                echo "- $entry"
            done
            echo ""
        } >> "$LEARNING_DIR/control_loop_tuning.md"
    fi
    
    # Update control loop configuration
    {
        echo "# Control Loop Configuration - Updated $TIMESTAMP"
        echo "last_execution: $TIMESTAMP"
        echo "build_status: $BUILD_STATUS"
        echo "audio_status: $AUDIO_TEST_STATUS"
        echo "memory_usage: ${MEMORY_USAGE}%"
        echo "learning_entries: ${#LEARNING_ENTRIES[@]}"
    } > control_loop_config.yaml
    
    echo "    ✅ Learning captured: ${#LEARNING_ENTRIES[@]} insights recorded" | tee -a "$LOG_FILE"
fi

# Summary and next iteration planning
echo "" | tee -a "$LOG_FILE"
echo "✅ Cybernetic loop iteration completed" | tee -a "$LOG_FILE"
echo "Summary:" | tee -a "$LOG_FILE"
echo "  • Build Status: $BUILD_STATUS" | tee -a "$LOG_FILE"
echo "  • Audio Status: $AUDIO_TEST_STATUS" | tee -a "$LOG_FILE"
echo "  • Learning Entries: ${#LEARNING_ENTRIES[@]}" | tee -a "$LOG_FILE"
echo "  • Execution Duration: $SECONDS seconds" | tee -a "$LOG_FILE"

NEXT_ITERATION=$(date -d '+1 hour' '+%Y-%m-%d %H:%M:%S')
echo "Next iteration scheduled for: $NEXT_ITERATION" | tee -a "$LOG_FILE"

# Final status determination
if [[ "$BUILD_STATUS" = "SUCCESS" || "$BUILD_STATUS" = "PASS" ]]; then
    echo "🎯 Overall Status: HEALTHY" | tee -a "$LOG_FILE"
    exit 0
else
    echo "⚠️  Overall Status: NEEDS ATTENTION" | tee -a "$LOG_FILE"
    exit 1
fi
