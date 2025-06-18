{
  description = "purple_garden is a lean lisp, designed and implemented with a focus on performance";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
  };
  outputs = { self, nixpkgs }: let
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};
  in {
    devShells.${system}.default = pkgs.mkShell {
      buildInputs = [
        pkgs.gcc
        pkgs.gnumake
        pkgs.clang-tools
        pkgs.hyperfine
        pkgs.valgrind
      ];
    };

    # this is missing git information since nix sucks ass and won't let me
    # include the .git folder required for COMMIT and COMMIT_MSG in the :w
    packages.${system}.default = pkgs.stdenv.mkDerivation {
        name = "purple-garden";
        src = ./.;
        buildInputs = [ pkgs.gcc pkgs.gnumake];
        buildPhase = "make release";
        installPhase = ''
            mkdir -p $out/bin
            cp ./build/purple_garden $out/bin/pg
        '';
    };
  };
}
