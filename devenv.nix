{ pkgs, lib, ... }:

{
  # Development environment name
  name = "rav-audio-visualizer";

  # Rust toolchain (rustc, cargo, rustfmt, clippy, rust-analyzer)
  languages.rust.enable = true;

  # Additional packages for development
  packages = with pkgs; [
    # Audio analysis tools for testing
    sox
    ffmpeg

    # Documentation
    mdbook

    # Visual testing and monitoring
    cargo-watch # For continuous testing

    # Nix linting, run by the test:nix task
    deadnix
    statix
    nixpkgs-fmt
  ] ++ lib.optionals stdenv.isLinux [
    # cpal links the ALSA backend on Linux, so nothing that compiles the audio
    # module works in this shell without these. macOS uses CoreAudio from the
    # SDK and needs nothing extra.
    pkg-config
    alsa-lib

    # Debugging and profiling (Linux only)
    gdb
    valgrind
    perf-tools
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
    if [ "$(uname -s)" != "Linux" ]; then
      echo "⚠️  perf is Linux-only; on macOS use Instruments or samply instead."
      exit 1
    fi
    cargo build --release --bin rav
    perf record -g ./target/release/rav
    perf report
  '';

  # Background Music (https://github.com/kyleneideck/BackgroundMusic) is what
  # exposes system audio as a capture device on macOS. It cannot be a `packages`
  # entry: it installs a CoreAudio HAL plug-in under /Library/Audio/Plug-Ins/HAL
  # plus a LaunchDaemon, all root-owned, so it can't be provided from the Nix
  # store by a project shell. This installs it the same way it already is here -
  # via Homebrew - and check-audio reports whether the driver is present.
  scripts.install-background-music.exec = ''
    if [ "$(uname -s)" != "Darwin" ]; then
      echo "⚠️  Background Music is macOS-only; on Linux use a PulseAudio/PipeWire monitor source."
      exit 1
    fi
    if [ -d "$BGM_DRIVER" ]; then
      echo "✅ Background Music is already installed."
      exit 0
    fi
    if ! command -v brew >/dev/null 2>&1; then
      echo "❌ Homebrew not found on PATH."
      echo "   Install it from https://brew.sh, or install Background Music manually:"
      echo "   https://github.com/kyleneideck/BackgroundMusic/releases"
      exit 1
    fi
    echo "📦 Installing Background Music (asks for sudo; restarts coreaudiod)..."
    brew install --cask background-music
  '';

  scripts.check-audio.exec = ''
    echo "🎵 Checking audio system..."
    if [ "$(uname -s)" = "Darwin" ]; then
      echo "CoreAudio input devices:"
      system_profiler SPAudioDataType 2>/dev/null || echo "Could not query CoreAudio"
      echo ""
      # rav prefers a CoreAudio process tap, which needs no virtual device and
      # captures ahead of the system volume. Background Music is only the
      # fallback now, so report the tap first or the message contradicts what
      # the app actually does.
      major=$(sw_vers -productVersion | cut -d. -f1)
      minor=$(sw_vers -productVersion | cut -d. -f2)
      if [ "$major" -gt 14 ] || { [ "$major" -eq 14 ] && [ "$minor" -ge 2 ]; }; then
        echo "✅ CoreAudio process tap available (macOS $major.$minor)."
        echo "   rav captures system audio directly - no virtual device needed."
        echo "   The first run asks for 'System Audio Recording Only' permission."
      else
        echo "⚠️  Process taps need macOS 14.2+; rav will fall back to a loopback device."
      fi
      echo ""
      # Check for the HAL driver, not the .app: the app can be present while the
      # driver that actually exposes the loopback input is not.
      if [ -d "$BGM_DRIVER" ]; then
        echo "   Fallback: Background Music installed."
      else
        echo "   Fallback: Background Music not installed ('install-background-music')."
      fi
    else
      echo "ALSA devices:"
      aplay -l || echo "aplay not available"
      echo ""
      echo "PulseAudio info:"
      pulseaudio --check -v || echo "PulseAudio not running"
      echo ""
      echo "Audio groups:"
      groups | grep -E 'audio|pulse' || echo "User not in audio groups"
    fi
  '';

  # Visual testing scripts
  scripts.visual-watch.exec = ''
    echo "🔄 Starting Cargo Watch for Visual Testing..."
    echo "This will rebuild and restart the visualizer on code changes"
    echo "Press Ctrl+C to stop"
    echo ""
    # --clean keeps log output out of the TUI; rav writes to rav.log instead.
    # That log lives in the repo root and is rewritten constantly, so it must be
    # ignored or every frame would retrigger a rebuild. .gitignore covers *.log
    # today and cargo-watch honours it, but the loop is bad enough to be worth
    # stating here rather than depending on that.
    cargo watch \
      -x "run --bin rav -- --clean" \
      --delay 2 \
      --clear \
      --why \
      --ignore "*.log" \
      --ignore "test_outputs/**" \
      --ignore "*.md" \
      --ignore "scripts/**"
  '';

  scripts.test-audio-pipeline.exec = ''
    echo "🧪 Testing audio pipeline functionality..."
    echo "This will validate that audio capture and processing work correctly"
    echo ""
    cargo run --bin test_audio_pipeline
  '';

  # Reproduces README.md's assets/demo.gif: screen-record a running rav window
  # and quantise it to a GIF. Everything is an env knob so a re-record lands at
  # the same size as the committed one and drops straight into the README.
  #
  # The recording is a guided tour rather than a static shot - it drives rav from
  # the outside while filming, so the GIF shows the three skins and then the
  # oscilloscope instead of one view for the whole clip. RAV_DEMO_TOUR is a list
  # of `seconds:key` steps: hold for that many seconds, then send that key.
  # `-` sends nothing, which is how the last segment gets its screen time.
  scripts.record-demo.exec = ''
    set -euo pipefail

    OUT="''${RAV_DEMO_OUT:-assets/demo.gif}"
    # The capture and the GIF run at the same rate. Grabbing above it and
    # sampling back down throws away real motion; grabbing below it cannot be
    # recovered by anything downstream. One variable, so they cannot drift.
    #
    # Set this to what the terminal actually repaints at; anything higher stores
    # duplicate frames. GIF holds delay in whole centiseconds, so the rates that
    # divide evenly are 25 (4cs), 50 (2cs) and 100 (1cs) - 60 lands on 1.67 and
    # comes out as an uneven 2/1 cadence.
    #
    # 25 is the macOS Terminal.app figure: measured by per-frame checksums it
    # repaints this content about 23 times a second. GPU-accelerated terminals
    # are far higher - WezTerm measured 52 on the same build - so on Linux set
    # RAV_DEMO_FPS=50 or 100.
    #
    # assets/demo.gif is currently a Terminal.app recording at 25. A Linux
    # re-record at a higher rate is pending.
    FPS="''${RAV_DEMO_FPS:-25}"
    LEAD="''${RAV_DEMO_LEAD:-5}"
    # Every skin, then the oscilloscope, then the help overlay - and back to the
    # defaults, so the GIF loops from the same frame it started on. There is one
    # `s` per skin: the last press is what returns to the first, and leaving it
    # out is how the tour quietly stops ending where it began.
    TOUR="''${RAV_DEMO_TOUR:-3:s 3:s 3:s 3:s 1:space 4:space 1:h 4:h 2:-}"
    # Total duration is the sum of the tour's holds, so the two never drift.
    # SETTLE covers the second avfoundation spends opening the capture device -
    # without it the first key lands before filming starts and that segment is
    # missing from the GIF.
    SETTLE="''${RAV_DEMO_SETTLE:-2}"
    DURATION=$(echo "$TOUR" | tr ' ' '\n' | awk -F: -v s="$SETTLE" '{t+=$1} END {print t + s}')
    # Points, not pixels. The committed GIF is a 941x249 window - the size that
    # gives the analyser a banner shape rather than a tall box. On a Retina
    # display set RAV_DEMO_SCALE=2, because the capture is in pixels while the
    # window geometry below is in points.
    SIZE="''${RAV_DEMO_SIZE:-941x249}"
    SCALE="''${RAV_DEMO_SCALE:-1}"

    WORK="$(mktemp -d)"
    trap 'rm -rf "$WORK"' EXIT
    mkdir -p "$(dirname "$OUT")"

    echo "🎬 record-demo -> $OUT"
    echo "   $DURATION s at $FPS fps, window $SIZE"
    echo "   tour: $TOUR"
    echo ""
    echo "Start rav, play something, and bring its window to the front."
    i="$LEAD"
    while [ "$i" -gt 0 ]; do
      printf '\r   recording in %s ... ' "$i"
      sleep 1
      i=$((i - 1))
    done
    printf '\r   recording now      \n'

    # WIDTHxHEIGHT+X+Y, in points. Read after the countdown so it describes the
    # window you just brought forward, not the shell you launched this from.
    GEOMETRY="''${RAV_DEMO_GEOMETRY:-}"

    if [ "$(uname -s)" = "Darwin" ]; then
      if [ -z "$GEOMETRY" ]; then
        GEOMETRY=$(osascript \
          -e 'tell application "System Events"' \
          -e 'set p to first application process whose frontmost is true' \
          -e 'set w to first window of p' \
          -e 'set {wx, wy} to position of w' \
          -e 'set {ww, wh} to size of w' \
          -e 'return (ww as text) & "x" & (wh as text) & "+" & (wx as text) & "+" & (wy as text)' \
          -e 'end tell' 2>/dev/null || true)
      fi
      if [ -z "$GEOMETRY" ]; then
        echo "❌ Could not read the front window's geometry."
        echo "   Grant Accessibility to your terminal in System Settings ->"
        echo "   Privacy & Security, or set RAV_DEMO_GEOMETRY=941x249+X+Y."
        exit 1
      fi

      # The screen's device index sits after every camera, so it moves when one
      # is plugged in. Read it rather than hardcoding it.
      #
      # `-list_devices` is a query with no input, so ffmpeg always exits non-zero.
      # Under `pipefail` that would end the script here; the `|| true` is what
      # makes the listing a query rather than a failure.
      SCREEN=$( { ffmpeg -hide_banner -f avfoundation -list_devices true -i "" 2>&1 || true; } \
        | sed -n 's/.*\[\([0-9][0-9]*\)\] Capture screen 0.*/\1/p' | head -1)
      if [ -z "$SCREEN" ]; then
        echo "❌ No screen capture device. Grant Screen Recording to your terminal"
        echo "   in System Settings -> Privacy & Security."
        exit 1
      fi
      # -pixel_format bgr0 is load-bearing: without it avfoundation negotiates
      # uyvy422 and chroma-subsamples the bars into a smear.
      GRAB=(-f avfoundation -capture_cursor 0 -pixel_format bgr0 -framerate "$FPS" -i "$SCREEN:none")
    else
      if [ -z "$GEOMETRY" ] && command -v xdotool >/dev/null 2>&1; then
        GEOMETRY=$(xdotool getactivewindow getwindowgeometry --shell \
          | awk -F= '/^X=/{x=$2} /^Y=/{y=$2} /^WIDTH=/{w=$2} /^HEIGHT=/{h=$2} END{print w "x" h "+" x "+" y}')
      fi
      if [ -z "$GEOMETRY" ]; then
        echo "❌ Set RAV_DEMO_GEOMETRY=941x249+X+Y (or install xdotool to read the"
        echo "   focused window automatically)."
        exit 1
      fi
    fi

    W=''${GEOMETRY%%x*}
    REST=''${GEOMETRY#*x}
    H=''${REST%%+*}
    REST=''${REST#*+}
    X=''${REST%%+*}
    Y=''${REST#*+}
    echo "   window ''${W}x''${H} at +''${X}+''${Y} (scale $SCALE)"

    CROP=$(awk -v s="$SCALE" -v w="$W" -v h="$H" -v x="$X" -v y="$Y" \
      'BEGIN { printf "crop=%d:%d:%d:%d", w * s, h * s, x * s, y * s }')

    # Send one keypress to whatever window is focused - which is rav's, since the
    # countdown just asked for it. Both of these need the same permission the
    # geometry read above already used.
    send_key() {
      case "$(uname -s)" in
      Darwin)
        if [ "$1" = "space" ]; then
          osascript -e 'tell application "System Events" to keystroke " "' >/dev/null 2>&1 || true
        else
          osascript -e "tell application \"System Events\" to keystroke \"$1\"" >/dev/null 2>&1 || true
        fi
        ;;
      *)
        command -v xdotool >/dev/null 2>&1 && xdotool key --clearmodifiers "$1" || true
        ;;
      esac
    }

    if [ "$(uname -s)" = "Darwin" ]; then
      ffmpeg -y -hide_banner -loglevel error "''${GRAB[@]}" \
        -t "$DURATION" -vf "$CROP" -c:v ffv1 "$WORK/raw.mkv" </dev/null &
    else
      ffmpeg -y -hide_banner -loglevel error \
        -f x11grab -framerate "$FPS" -video_size "''${W}x''${H}" \
        -i "''${DISPLAY}+''${X},''${Y}" \
        -t "$DURATION" -c:v ffv1 "$WORK/raw.mkv" </dev/null &
    fi
    GRABBER=$!

    # Walk the tour alongside the capture. ffmpeg stops itself at -t, so the two
    # finish together without either having to know about the other.
    sleep "$SETTLE"
    for step in $TOUR; do
      sleep "''${step%%:*}"
      key="''${step#*:}"
      [ "$key" = "-" ] || send_key "$key"
    done
    wait "$GRABBER"

    # Two passes: one palette for the whole clip, then quantise against it. A
    # per-frame palette makes the green-to-red ramp crawl between frames.
    echo "🎨 Quantising..."
    # No `fps` filter: the capture is already at $FPS, and resampling a stream to
    # the rate it is already at is how frames get dropped or doubled for nothing.
    ffmpeg -y -v error -i "$WORK/raw.mkv" \
      -vf "palettegen=stats_mode=full:max_colors=128" "$WORK/palette.png"
    ffmpeg -y -v error -i "$WORK/raw.mkv" -i "$WORK/palette.png" \
      -lavfi "[0:v][1:v]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle" \
      "$OUT"

    echo "✅ $OUT ($(du -h "$OUT" | cut -f1))"
  '';

  # rustfmt only. devenv wires `devenv:git-hooks:run` before
  # `devenv:enterTest`, so every hook also runs inside `devenv test` and
  # therefore in CI. A clippy hook there would use its own default flags - no
  # --all-targets, no --all-features, warnings not denied - which is a different
  # cargo fingerprint from the test:clippy task, so it compiles the crate under
  # clippy a second time and cannot fail anything that task does not.
  # Formatting is the part worth catching before the commit rather than after.
  git-hooks.hooks = {
    rustfmt.enable = true;
  };

  # Environment variables
  env = {
    PROJECT_NAME = "rav";
    RUST_LOG = "rav=debug,info";
    CARGO_TARGET_DIR = "./target";
    # Where CoreAudio loads the Background Music loopback driver from. Only
    # meaningful on macOS; harmless elsewhere.
    BGM_DRIVER = "/Library/Audio/Plug-Ins/HAL/Background Music Device.driver";
  };

  # Development shell setup.
  #
  # Everything the banner prints goes to stderr. `devenv shell -- <cmd>` runs
  # this first, so anything on stdout is prepended to that command's output -
  # and CI runs `devenv shell -- cargo metadata` (Swatinem/rust-cache's
  # cmd-format probe), where a banner in front of the JSON is unparseable. The
  # action swallows that error, keeps the step green and caches nothing.
  enterShell = ''
    {
      echo "⚡ RAV development environment loaded"
      echo "Available scripts:"
      echo "  • dev-test           - Run tests"
      echo "  • dev-run            - Run the visualizer"
      echo "  • dev-profile        - Profile performance"
      echo "  • check-audio        - Verify audio setup"
      echo "  • install-background-music - Install macOS system-audio capture"
      echo ""
      echo "Visual Testing:"
      echo "  • visual-watch       - Continuous testing with cargo watch"
      echo "  • test-audio-pipeline - Test audio capture functionality"
      echo "  • record-demo        - Screen-record the README demo GIF"
      echo ""
      check-audio
    } >&2
  '';

  # These tasks are the check suite. `devenv test` runs them, and so does CI, so
  # a local run and a CI run execute the same thing. Add a check by adding a
  # task here, never by adding a step to the workflow.
  #
  # The `after` edges are load-bearing, not decoration: devenv runs tasks with
  # no edge between them concurrently (measured - two independent sleep tasks
  # start at the same instant), so without these, fmt, clippy and test all
  # invoke cargo at once and block on the single lock over ./target. Serialised,
  # they run cheapest-first - a formatting or nix-lint failure costs no
  # compilation at all - and test:unit reuses the dependency artifacts clippy
  # just built, since clippy goes through RUSTC_WORKSPACE_WRAPPER and only
  # workspace crates take the clippy-driver path.
  tasks = {
    "test:fmt".exec = "cargo fmt --all --check";
    # Tracked files only. `target/` holds unpacked copies of previously packaged
    # releases, and linting those reports findings against an old release.
    "test:nix".exec = ''
      set -eu
      files=$(git ls-files '*.nix')
      deadnix --fail $files
      nixpkgs-fmt --check $files
      statix check .
    '';
    "test:clippy" = {
      exec = "cargo clippy --all-targets --all-features -- -D warnings";
      after = [ "test:fmt" "test:nix" ];
    };
    "test:unit" = {
      exec = "cargo test --all-features";
      after = [ "test:clippy" ];
    };
  };

  enterTest = lib.mkForce "devenv tasks run test:fmt test:clippy test:nix test:unit";
}
