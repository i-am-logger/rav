#!/bin/bash

# Visual Watch Test Script for Speedy v1.0
# This script uses cargo watch to continuously monitor and test the visualizer
# capturing visual outputs for analysis

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="$PROJECT_DIR/test_outputs/visual_watch"

# Create output directory
mkdir -p "$OUTPUT_DIR"

echo "🔍 Visual Watch Test for Speedy v1.0"
echo "===================================="
echo "Output directory: $OUTPUT_DIR"
echo

# Function to capture terminal output with ttyrec/script
capture_visual_output() {
    local test_name="$1"
    local duration="$2"
    local output_file="$OUTPUT_DIR/${test_name}_$(date +%Y%m%d_%H%M%S)"
    
    echo "📺 Capturing visual output: $test_name"
    echo "Duration: ${duration}s"
    echo "Output: $output_file"
    
    # Use script command to capture terminal session
    timeout "$duration" script -q -c "nix develop -c cargo run --bin speedy" "$output_file.txt" || true
    
    # Also try to capture with ANSI sequences preserved
    timeout "$duration" nix develop -c cargo run --bin speedy 2>&1 | tee "$output_file.ansi" || true
    
    echo "✅ Captured: $output_file"
    echo
}

# Function to run with specific audio scenarios
test_with_audio_scenario() {
    local scenario="$1"
    local duration="$2"
    
    echo "🎵 Testing scenario: $scenario"
    
    case "$scenario" in
        "silent")
            echo "Testing with no audio input..."
            capture_visual_output "scenario_silent" "$duration"
            ;;
        "demo")
            echo "Testing with demo mode (auto-generated patterns)..."
            capture_visual_output "scenario_demo" "$duration"
            ;;
        "live")
            echo "Testing with live audio (play some music now!)..."
            echo "▶️  Please start playing audio now for live testing..."
            sleep 3
            capture_visual_output "scenario_live_audio" "$duration"
            ;;
        *)
            echo "Unknown scenario: $scenario"
            ;;
    esac
}

# Function to analyze captured output
analyze_output() {
    local output_file="$1"
    
    echo "🔍 Analyzing output: $(basename "$output_file")"
    
    if [[ -f "$output_file" ]]; then
        # Basic analysis
        local line_count=$(wc -l < "$output_file")
        local size=$(du -h "$output_file" | cut -f1)
        
        echo "  Lines: $line_count"
        echo "  Size: $size"
        
        # Check for specific patterns
        if grep -q "🚀 Switched to Professional UI" "$output_file"; then
            echo "  ✅ Professional UI activated"
        else
            echo "  ⚠️  Professional UI not detected"
        fi
        
        if grep -q "🎭 Activating demo mode" "$output_file"; then
            echo "  ✅ Demo mode activated"
        else
            echo "  ℹ️  No demo mode detected"
        fi
        
        if grep -q "Audio capture started" "$output_file"; then
            echo "  ✅ Audio capture working"
        else
            echo "  ❌ Audio capture issues"
        fi
        
        # Check for errors
        local error_count=$(grep -c "ERROR\|error\|Error\|panic\|PANIC" "$output_file" 2>/dev/null || echo "0")
        if [[ "$error_count" -gt 0 ]]; then
            echo "  ❌ Errors found: $error_count"
        else
            echo "  ✅ No errors detected"
        fi
        
        echo
    else
        echo "  ❌ Output file not found"
    fi
}

# Function to run cargo watch with visual monitoring
run_cargo_watch() {
    echo "🔄 Starting cargo watch with visual monitoring..."
    echo "This will rebuild and restart the visualizer on code changes"
    echo "Press Ctrl+C to stop"
    echo
    
    # Set up cargo watch to monitor and restart
    cargo watch -x "run --bin speedy" \
        --why \
        --delay 2 \
        --ignore "test_outputs/**" \
        --ignore "*.md" \
        --ignore "scripts/**" || true
}

# Main execution
main() {
    cd "$PROJECT_DIR"
    
    echo "Select test mode:"
    echo "1) Single visual capture test (10s)"
    echo "2) Multiple scenario testing"
    echo "3) Cargo watch mode (continuous)"
    echo "4) Quick demo test (5s)"
    echo
    read -p "Choose option (1-4): " choice
    
    case "$choice" in
        "1")
            echo "Running single visual capture test..."
            capture_visual_output "single_test" 10
            analyze_output "$OUTPUT_DIR/single_test_$(date +%Y%m%d)*.txt"
            ;;
        "2")
            echo "Running multiple scenario tests..."
            test_with_audio_scenario "silent" 5
            test_with_audio_scenario "demo" 8
            test_with_audio_scenario "live" 10
            
            echo "📊 Analysis Results:"
            echo "==================="
            for file in "$OUTPUT_DIR"/*.txt; do
                if [[ -f "$file" ]]; then
                    analyze_output "$file"
                fi
            done
            ;;
        "3")
            run_cargo_watch
            ;;
        "4")
            echo "Running quick demo test..."
            capture_visual_output "quick_demo" 5
            analyze_output "$OUTPUT_DIR/quick_demo_$(date +%Y%m%d)*.txt"
            ;;
        *)
            echo "Invalid choice. Running quick demo test..."
            capture_visual_output "quick_demo" 5
            ;;
    esac
}

# Check dependencies
check_deps() {
    local missing_deps=()
    
    if ! command -v cargo &> /dev/null; then
        missing_deps+=("cargo")
    fi
    
    if ! command -v script &> /dev/null; then
        missing_deps+=("script")
    fi
    
    if ! command -v timeout &> /dev/null; then
        missing_deps+=("timeout")
    fi
    
    if [[ ${#missing_deps[@]} -gt 0 ]]; then
        echo "❌ Missing dependencies: ${missing_deps[*]}"
        echo "Please install missing dependencies and try again"
        exit 1
    fi
}

# Install cargo watch if not present
install_cargo_watch() {
    if ! command -v cargo-watch &> /dev/null; then
        echo "📦 Installing cargo-watch..."
        nix develop -c cargo install cargo-watch
    fi
}

# Initialize
check_deps
install_cargo_watch
main "$@"
