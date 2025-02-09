{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default";
    fenix.url = "github:nix-community/fenix";
    fenix.inputs.nixpkgs.follows = "nixpkgs";
    naersk.url = "github:nix-community/naersk";
    naersk.inputs.nixpkgs.follows = "nixpkgs";
    nixpkgs-mozilla = {
      url = "github:mozilla/nixpkgs-mozilla";
      flake = false;
    };

    devenv.url = "github:cachix/devenv";
    devenv.inputs.nixpkgs.follows = "nixpkgs";
  };

  nixConfig = {
    extra-trusted-public-keys = "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw=";
    extra-substituters = "https://devenv.cachix.org";
  };

  outputs =
    {
      self,
      nixpkgs,
      fenix,
      devenv,
      systems,
      naersk,
      nixpkgs-mozilla,
      ...
    }@inputs:
    let
      forEachSystem = nixpkgs.lib.genAttrs (import systems);
      lib = nixpkgs.lib;
    in
    {
      packages = forEachSystem (
        system:
        let
          pkgs = (import nixpkgs) {
            inherit system;

            overlays = [
              (import nixpkgs-mozilla)
            ];
          };

          toolchain =
            (pkgs.rustChannelOf {
              rustToolchain = ./rust-toolchain.toml;
              sha256 = "sha256-vFu6RmeJsrTgIjNjNJrC+pVZh1fgr0wm7VX24RJQ14k=";
            }).rust;

          package = pkgs.callPackage ./package.nix {
            inherit
              pkgs
              lib
              naersk
              toolchain
              ;
          };
          devenv-up = self.devShells.${system}.default.config.procfileScript;
          devenv-test = self.devShells.${system}.default.config.test;
        in
        {
          inherit
            package
            devenv-up
            devenv-test
            ;
          default = package;
        }
      );

      devShells = forEachSystem (
        system:
        let
          pkgs = (import nixpkgs) {
            inherit system;

            overlays = [
              (import nixpkgs-mozilla)
            ];
          };
        in
        {
          default = import ./devenv.nix {
            inherit
              inputs
              pkgs
              ;
          };
        }
      );
    };
}
