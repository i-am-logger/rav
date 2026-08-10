{
  description = "RAV - Rust Audio Visualizer Development Environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachSystem [
      "x86_64-linux"
      "aarch64-linux"
      "aarch64-darwin"
    ]
      (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          inherit (pkgs) lib stdenv;
          # Cargo.toml is the single source of truth for the version. release-plz
          # bumps it on release, and reading it here is what keeps the flake from
          # drifting out of sync with it.
          cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

          rav = pkgs.rustPlatform.buildRustPackage {
            pname = "rav";
            version = cargoToml.package.version;
            src = ./.;
            cargoLock = {
              lockFile = ./Cargo.lock;
            };
            # pkg-config/alsa-lib are only needed to link cpal's ALSA backend on
            # Linux. On macOS cpal uses CoreAudio, which comes from the SDK in
            # stdenv, so there is nothing extra to add.
            nativeBuildInputs = lib.optionals stdenv.isLinux [ pkgs.pkg-config ];
            buildInputs = lib.optionals stdenv.isLinux [ pkgs.alsa-lib ];

            # buildRustPackage runs cargo test by default. The test job already
            # does, with a cargo cache this build cannot use, so this exists to
            # prove packaging rather than to re-prove the tests.
            doCheck = false;

            # `nix run` falls back to pname when this is unset, which happens to be
            # right here - but only by coincidence. Stating it means adding or
            # renaming a binary cannot silently change what `nix run` executes.
            meta.mainProgram = "rav";
          };
        in
        {
          packages.default = rav;

          # `nix flake check` builds the checks and only evaluates packages, so
          # exposing the package here is what makes one command cover the build.
          checks.package = rav;

          devShells.default = pkgs.mkShell {
            buildInputs = (with pkgs; [
              # Rust toolchain
              rustc
              cargo
              rustfmt
              rust-analyzer
              clippy

              # Development tools
              pkg-config

              # Build tools
              cmake
              gnumake

              # Audio analysis tools for testing
              sox
              ffmpeg

              # Visual testing and monitoring
              cargo-watch # For continuous testing
            ]) ++ lib.optionals stdenv.isLinux (with pkgs; [
              # Audio development libraries (Linux backends for cpal)
              alsa-lib
              alsa-utils

              # System libraries often needed for audio
              udev

              # Debugging tools
              gdb
            ]);

            shellHook = ''
              ${lib.optionalString stdenv.isLinux ''
                # Interpolating alsa-lib forces it to evaluate, and it does not
                # evaluate on darwin - so this has to be guarded here too, not just
                # in buildInputs above.
                export PKG_CONFIG_PATH="${pkgs.alsa-lib.dev}/lib/pkgconfig:$PKG_CONFIG_PATH"
                export LD_LIBRARY_PATH="${pkgs.alsa-lib}/lib:$LD_LIBRARY_PATH"
              ''}
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
            
              test-audio-pipeline() {
                echo "🧪 Testing audio pipeline functionality..."
                echo "This will validate that audio capture and processing work correctly"
                echo ""
                cargo run --bin test_audio_pipeline
              }

              echo "🚀 RAV Development Environment"
              echo "📦 Rust version: $(rustc --version)"
              echo "🔧 Cargo version: $(cargo --version)"
              echo ""
              echo "Available audio devices:"
              if [ "$(uname -s)" = "Darwin" ]; then
                echo "🎵 Audio backend: CoreAudio (system audio via Background Music)"
                system_profiler SPAudioDataType 2>/dev/null | grep -E '^        [A-Za-z]' || echo "Could not query CoreAudio"
              else
                echo "🎵 Audio backend: ALSA (system audio via a PulseAudio/PipeWire monitor source)"
                aplay -l 2>/dev/null || echo "No audio playback devices found"
              fi
              echo ""
              echo "Visual Testing Functions:"
              echo "  • visual-watch       - Continuous testing with cargo watch"
              echo "  • test-audio-pipeline - Test audio capture functionality"
              echo ""
              echo "This shell is the fallback for plain 'nix develop'; the project"
              echo "itself uses devenv (see .envrc), which also has record-demo."
            '';
          };
        });
}
