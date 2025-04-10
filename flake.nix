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
  };
}
