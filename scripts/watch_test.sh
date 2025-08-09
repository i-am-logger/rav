#!/usr/bin/env bash

# Cargo watch configuration for comprehensive audio visualizer testing
# This script runs continuous testing with the custom audio monitor device

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="$PROJECT_DIR/test_results/watch_results"

# Create output directory
mkdir -p "$OUTPUT_DIR"

echo "🚀 Starting Cargo Watch with Audio Monitor Testing"
echo "===================================================="
echo "Project: $PROJECT_DIR"
echo "Output: $OUTPUT_DIR"
echo ""

# Function to run tests with clean output
run_clean_tests() {
    echo "🔄 Code change detected, running tests..."
    
    # First check compilation
    if ! cargo check --quiet; then
        echo "❌ Compilation failed, skipping tests"
        return 1
    fi
    
    echo "✅ Compilation successful, starting audio tests..."
    
    # Run the frequency test suite in continuous mode
    cargo run --bin frequency_test_suite -- \
        --continuous \
        --profile sweep \
        --duration 25 \
        --output-dir "$OUTPUT_DIR" \
        2>/dev/null &
    
    local test_pid=$!
    
    # Let it run for a few seconds then stop
    sleep 5
    kill $test_pid 2>/dev/null || true
    
    echo "🎯 Quick test cycle completed"
    echo ""
}

# Function to run comprehensive tests
run_comprehensive_tests() {
    echo "🎯 Running comprehensive test suite..."
    
    # Run all test profiles quickly
    cargo run --bin frequency_test_suite -- \
        --profile all \
        --duration 50 \
        --output-dir "$OUTPUT_DIR" \
        --detailed-report
    
    echo "📊 Comprehensive tests completed, results in $OUTPUT_DIR"
}

# Function to run clean visualizer for manual inspection
run_clean_visualizer() {
    echo "🎨 Starting clean visualizer (use Ctrl+C to stop)..."
    
    # Run with clean mode to avoid log pollution
    cargo run --bin speedy -- --clean &
    local app_pid=$!
    
    # Also run audio monitor to provide test tones
    cargo run --bin frequency_test_suite -- \
        --continuous \
        --profile bass \
        --duration 200 \
        2>/dev/null &
    local audio_pid=$!
    
    # Wait for user to stop
    wait $app_pid
    
    # Stop audio monitor
    kill $audio_pid 2>/dev/null || true
}

# Check for required tools
check_dependencies() {
    if ! command -v cargo-watch &> /dev/null; then
        echo "📦 Installing cargo-watch..."
        cargo install cargo-watch
    fi
    
    if ! command -v cargo &> /dev/null; then
        echo "❌ Cargo not found! Please install Rust toolchain"
        exit 1
    fi
}

# Main execution based on arguments
main() {
    check_dependencies
    
    case "${1:-watch}" in
        "watch")
            echo "🔄 Starting cargo watch mode..."
            echo "This will run quick tests on every file change"
            echo "Press Ctrl+C to stop"
            echo ""
            
            # Use cargo watch to monitor file changes and run tests
            cargo watch \
                --why \
                --delay 2 \
                --ignore "test_results/**" \
                --ignore "*.md" \
                --ignore "scripts/**" \
                --shell 'bash -c "cd '"$PROJECT_DIR"' && bash '"$0"' test"'
            ;;
            
        "test")
            # Called by cargo watch
            run_clean_tests
            ;;
            
        "comprehensive")
            run_comprehensive_tests
            ;;
            
        "visual")
            run_clean_visualizer
            ;;
            
        "help")
            echo "Usage: $0 [command]"
            echo ""
            echo "Commands:"
            echo "  watch         - Monitor files and run tests on changes (default)"
            echo "  test          - Run quick test cycle (used internally)"
            echo "  comprehensive - Run full test suite once"
            echo "  visual        - Start clean visualizer with test audio"
            echo "  help          - Show this help"
            echo ""
            echo "Examples:"
            echo "  $0                    # Start watching for changes"
            echo "  $0 comprehensive      # Run all tests once"
            echo "  $0 visual             # Manual visual inspection"
            ;;
            
        *)
            echo "❌ Unknown command: $1"
            echo "Use '$0 help' for usage information"
            exit 1
            ;;
    esac
}

# Trap to cleanup background processes
cleanup() {
    echo ""
    echo "🧹 Cleaning up background processes..."
    jobs -p | xargs -r kill 2>/dev/null || true
    exit 0
}

trap cleanup INT TERM

# Run main function with all arguments
main "$@"
