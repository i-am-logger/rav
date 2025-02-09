{
  pkgs,
  naersk,
  toolchain,
  ...
}:

let
  naersk' = pkgs.callPackage naersk {
    cargo = toolchain;
    rustc = toolchain;
  };
in
naersk'.buildPackage {
  pname = "rav";
  src = ./.;
  doCheck = true;
  release = true;

  # runtime dependencies
  nativeBuildInputs = with pkgs; [
    pkg-config
    openssl
    alsa-lib
  ];

  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (
    with pkgs;
    [
      alsa-lib
    ]
  );

  # build dependencies
  buildInputs = with pkgs; [
    curl
    openssl.dev
    alsa-lib.dev
  ];

  # Override the install phase to ensure proper runtime paths
  overrideMain = attrs: {
    postInstall = ''
      patchelf --set-rpath "${
        pkgs.lib.makeLibraryPath (
          with pkgs;
          [
            alsa-lib
            stdenv.cc.cc.lib
          ]
        )
      }" $out/bin/rav
    '';
  };
}
