{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  name = "tunotron-dev";
  buildInputs = with pkgs; [
    rustc
    cargo
    rustfmt
    clippy
    mpv
    pkg-config
  ];

  shellHook = ''
    echo "🎵 Welcome to Tunotron development environment!"
  '';
}
