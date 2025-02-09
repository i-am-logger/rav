{
  languages = {
    rust = {
      enable = true;
      # channel = "nixpkgs";
      channel = "nightly";
      components = [
        "rustc"
        "cargo"
        "clippy"
        "rustfmt"
        "rust-analyzer"
      ];
    };
  };

  git-hooks.hooks = {
    clippy.enable = true;
    rustfmt.enable = true;
  };

  enterTest = ''
    cargo test
  '';
}
