{
  inputs,
  pkgs,
  ...
}:
inputs.devenv.lib.mkShell {
  inherit
    inputs
    pkgs
    ;
  modules = [
    (import ./nix/languages/rust.nix)
    (import ./nix/processes/cargo-watch.nix)
    {
      packages = with pkgs; [
        cargo-watch
        pkg-config
        alsa-lib.dev
        figlet
      ];

      dotenv.enable = true;

      enterShell = ''
        figlet "rav"
        echo "Rust Audio Visualizer devenv"
        echo "============================"
      '';
    }
  ];
}
