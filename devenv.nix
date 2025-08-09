{ pkgs, ... }:

{
  # Development environment name
  name = "rav-audio-visualizer";

  # Additional packages for development
  packages = with pkgs; [
    # Audio analysis tools for testing
    sox
    ffmpeg
    
    # Debugging and profiling
    gdb
    valgrind
    perf-tools
    
    # Documentation
    mdbook
    
    # Visual testing and monitoring
    cargo-watch    # For continuous testing
    script         # For terminal session capture
    util-linux     # Contains script command
  ];

  # Development scripts
  scripts.dev-test.exec = ''
    echo "🧪 Running development tests..."
    cargo test --all-features
  '';

  scripts.dev-run.exec = ''
    echo "🚀 Running RAV audio visualizer..."
    cargo run --release --bin rav
  '';

  scripts.dev-profile.exec = ''
    echo "📊 Profiling application performance..."
    cargo build --release --bin rav
    perf record -g ./target/release/rav
    perf report
  '';

  scripts.check-audio.exec = ''
    echo "🎵 Checking audio system..."
    echo "ALSA devices:"
    aplay -l
    echo ""
    echo "PulseAudio info:"
    pulseaudio --check -v || echo "PulseAudio not running"
    echo ""
    echo "Audio groups:"
    groups | grep -E 'audio|pulse' || echo "User not in audio groups"
  '';

  # Visual testing scripts
  scripts.visual-watch.exec = ''
    echo "🔄 Starting Cargo Watch for Visual Testing..."
    echo "This will rebuild and restart the visualizer on code changes"
    echo "Press Ctrl+C to stop"
    echo ""
    cargo watch \
      -x "run --bin rav" \
      --delay 2 \
      --clear \
      --why \
      --ignore "test_outputs/**" \
      --ignore "*.md" \
      --ignore "scripts/**"
  '';

  scripts.visual-capture.exec = ''
    echo "📺 Capturing visual output for analysis..."
    OUTPUT_DIR="test_outputs/visual_watch"
    mkdir -p "$OUTPUT_DIR"
    TIMESTAMP=$(date +%Y%m%d_%H%M%S)
    OUTPUT_FILE="$OUTPUT_DIR/visual_capture_$TIMESTAMP"
    
    echo "Duration: 10 seconds"
    echo "Output will be saved to: $OUTPUT_FILE.txt"
    echo ""
    echo "Starting visualizer - play some audio now!"
    timeout 10 script -q -c "cargo run --bin rav" "$OUTPUT_FILE.txt" || true
    
    echo "✅ Capture completed: $OUTPUT_FILE.txt"
    echo "Run 'visual-analyze $OUTPUT_FILE.txt' to analyze the output"
  '';

  scripts.visual-analyze.exec = ''
    if [ $# -eq 0 ]; then
      echo "❌ Usage: visual-analyze <captured_output_file>"
      echo "Example: visual-analyze test_outputs/visual_watch/visual_capture_20250109_014700.txt"
      exit 1
    fi
    
    if [ ! -f "$1" ]; then
      echo "❌ File not found: $1"
      exit 1
    fi
    
    echo "🔍 Analyzing visual output: $1"
    cargo run --bin visual_analyzer "$1"
  '';

  scripts.test-audio-pipeline.exec = ''
    echo "🧪 Testing audio pipeline functionality..."
    echo "This will validate that audio capture and processing work correctly"
    echo ""
    cargo run --bin test_audio_pipeline
  '';

  scripts.comprehensive-test.exec = ''
    echo "🧪 Running comprehensive visual and audio tests..."
    echo "==============================================="
    echo ""
    
    # Run BDD tests
    echo "1. Running BDD test suite..."
    cargo run --bin comprehensive_bdd_test
    echo ""
    
    # Test audio pipeline
    echo "2. Testing audio pipeline..."
    cargo run --bin test_audio_pipeline
    echo ""
    
    # Capture visual output
    echo "3. Capturing visual output..."
    visual-capture
    echo ""
    
    echo "✅ Comprehensive testing completed!"
    echo "Check test_outputs/ directory for results"
  '';

  # Git hooks for development
  pre-commit.hooks = {
    rustfmt.enable = true;
    clippy.enable = true;
  };

  # Environment variables
  env = {
    PROJECT_NAME = "rav";
    RUST_LOG = "rav=debug,info";
    CARGO_TARGET_DIR = "./target";
  };

  # Development shell setup
  enterShell = ''
    echo "⚡ RAV development environment loaded"
    echo "Available scripts:"
    echo "  • dev-test           - Run tests"
    echo "  • dev-run            - Run the visualizer"  
    echo "  • dev-profile        - Profile performance"
    echo "  • check-audio        - Verify audio setup"
    echo ""
    echo "Visual Testing:"
    echo "  • visual-watch       - Continuous testing with cargo watch"
    echo "  • visual-capture     - Capture 10s of visual output"
    echo "  • visual-analyze     - Analyze captured output"
    echo "  • test-audio-pipeline - Test audio capture functionality"
    echo "  • comprehensive-test - Run all tests and capture output"
    echo ""
    check-audio
  '';
}
