{
  description = "Arreat Index Rust workspace";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/64c08a7ca051951c8eae34e3e3cb1e202fe36786";
    fenix = {
      url = "github:nix-community/fenix/9a4f7863c93539f17a1785962d58471fdc7f5fe4";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, fenix }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      rustToolchain = fenix.packages.${system}.stable.withComponents [
        "cargo"
        "clippy"
        "rustc"
        "rustfmt"
        "rust-std"
      ];
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = [
          rustToolchain
          pkgs.git
          pkgs.curl
          pkgs.jq
          pkgs.pkg-config
          pkgs.stdenv.cc
        ];
      };

      checks.${system}.workspace = pkgs.stdenv.mkDerivation {
        pname = "arreat-index-workspace";
        version = "0.1.0";
        src = self;
        nativeBuildInputs = [ rustToolchain ];
        buildPhase = ''
          runHook preBuild
          export HOME="$TMPDIR"
          cargo build --workspace --all-targets --locked
          cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
          cargo fmt --all -- --check
          runHook postBuild
        '';
        doCheck = true;
        checkPhase = ''
          runHook preCheck
          cargo test --workspace --all-targets --locked
          runHook postCheck
        '';
        installPhase = ''
          runHook preInstall
          mkdir -p "$out"
          touch "$out/verified"
          runHook postInstall
        '';
      };
    };
}
