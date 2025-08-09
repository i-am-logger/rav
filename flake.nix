{
  description = "RAV - Rust Audio Visualizer Development Environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "rav";
          version = "0.3.0";
          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = with pkgs; [
            alsa-lib
            pulseaudio
            jack2
            portaudio
            fftw
          ];
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Rust toolchain
            rustc
            cargo
            rustfmt
            rust-analyzer
            clippy

            # Audio development libraries
            alsa-lib
            alsa-utils
            pulseaudio
            jack2
            portaudio
            
            # Development tools
            pkg-config
            gcc
            
            # System libraries often needed for audio
            udev
            
            # Optional: for advanced audio processing
            fftw
            
            # Build tools
            cmake
            gnumake
            
            # Audio analysis tools for testing
            sox
            ffmpeg
            
            # Debugging tools
            gdb
            
            # Visual testing and monitoring
            cargo-watch    # For continuous testing
            util-linux     # Contains script command for terminal capture
          ];

          shellHook = ''
            export PKG_CONFIG_PATH="${pkgs.alsa-lib.dev}/lib/pkgconfig:${pkgs.pulseaudio.dev}/lib/pkgconfig:$PKG_CONFIG_PATH"
            export LD_LIBRARY_PATH="${pkgs.alsa-lib}/lib:${pkgs.pulseaudio}/lib:${pkgs.jack2}/lib:$LD_LIBRARY_PATH"
            export RUST_BACKTRACE=1
            export RUST_LOG="rav=debug,info"
            
            # Create visual testing functions
            visual-watch() {
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
            }
            
            visual-capture() {
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
            }
            
            visual-analyze() {
              if [ $# -eq 0 ]; then
                echo "❌ Usage: visual-analyze <captured_output_file>"
                echo "Example: visual-analyze test_outputs/visual_watch/visual_capture_20250109_014700.txt"
                return 1
              fi
              
              if [ ! -f "$1" ]; then
                echo "❌ File not found: $1"
                return 1
              fi
              
              echo "🔍 Analyzing visual output: $1"
              cargo run --bin visual_analyzer "$1"
            }
            
            test-audio-pipeline() {
              echo "🧪 Testing audio pipeline functionality..."
              echo "This will validate that audio capture and processing work correctly"
              echo ""
              cargo run --bin test_audio_pipeline
            }
            
            comprehensive-test() {
              echo "🧪 Running comprehensive visual and audio tests..."
              echo "==============================================="
              echo ""
              
              # Run BDD tests
              echo "1. Running BDD test suite..."
              cargo run --bin comprehensive_bdd_test
              echo ""
              
              # Test audio pipeline
              echo "2. Testing audio pipeline..."
              test-audio-pipeline
              echo ""
              
              # Capture visual output
              echo "3. Capturing visual output..."
              visual-capture
              echo ""
              
              echo "✅ Comprehensive testing completed!"
              echo "Check test_outputs/ directory for results"
            }
            
            echo "🚀 RAV Development Environment"
            echo "📦 Rust version: $(rustc --version)"
            echo "🔧 Cargo version: $(cargo --version)"
            echo "🎵 Audio libraries: ALSA, PulseAudio, JACK available"
            echo ""
            echo "Available audio devices:"
            aplay -l 2>/dev/null || echo "No audio playback devices found"
            echo ""
            echo "Visual Testing Functions:"
            echo "  • visual-watch       - Continuous testing with cargo watch"
            echo "  • visual-capture     - Capture 10s of visual output"
            echo "  • visual-analyze     - Analyze captured output"
            echo "  • test-audio-pipeline - Test audio capture functionality"
            echo "  • comprehensive-test - Run all tests and capture output"
            echo ""
            echo "⚡ Ready to build advanced audio visualizer with ratatui!"
          '';
        };
      });
}
