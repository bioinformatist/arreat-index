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
      rustPlatform = pkgs.makeRustPlatform { cargo = rustToolchain; rustc = rustToolchain; };
      workspaceCargoDeps = rustPlatform.fetchCargoVendor {
        src = self;
        hash = "sha256-OrBQiDfNAmNWgN/Fqa/qzRgMZxRGcPGoVcUHalOlGvQ=";
      };
      cascLib = pkgs.stdenv.mkDerivation {
        pname = "casclib";
        version = "3.0";
        src = pkgs.fetchFromGitHub {
          owner = "ladislav-zezula";
          repo = "CascLib";
          rev = "4971d363e665551ac4142f541e5f2d71f1cda653";
          hash = "sha256-NTFENbLjU3oapo1IAwqC86EtQ8F+4JN0POat9csi3Pk=";
        };
        nativeBuildInputs = [ pkgs.cmake ];
        buildInputs = [ pkgs.zlib ];
        cmakeFlags = [
          "-DCMAKE_POLICY_VERSION_MINIMUM=3.5"
          "-DCASC_BUILD_SHARED_LIB=OFF"
          "-DCASC_BUILD_STATIC_LIB=ON"
        ];
        meta.license = pkgs.lib.licenses.mit;
      };
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = [
          rustToolchain
          pkgs.git
          pkgs.curl
          pkgs.jq
          pkgs.opencc
          pkgs.check-jsonschema
          pkgs.pkg-config
          pkgs.stdenv.cc
          cascLib
          pkgs.zlib
          pkgs.zlib.static
          pkgs.cacert
        ];
        CASCLIB_LIB_DIR = "${cascLib}/lib";
        ZLIB_STATIC_LIB_DIR = "${pkgs.zlib.static}/lib";
        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ pkgs.stdenv.cc.cc.lib ];
        SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
      };

      checks.${system}.workspace = pkgs.stdenv.mkDerivation {
        pname = "arreat-index-workspace";
        version = "0.1.0";
        src = self;
        nativeBuildInputs = [ rustToolchain rustPlatform.cargoSetupHook ];
        buildInputs = [ cascLib pkgs.zlib pkgs.zlib.static pkgs.stdenv.cc.cc.lib pkgs.cacert ];
        cargoDeps = workspaceCargoDeps;
        CASCLIB_LIB_DIR = "${cascLib}/lib";
        ZLIB_STATIC_LIB_DIR = "${pkgs.zlib.static}/lib";
        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ pkgs.stdenv.cc.cc.lib ];
        SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
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

      packages.${system}.arreat-data-static = pkgs.stdenv.mkDerivation {
        pname = "arreat-data-static";
        version = "0.1.0";
        src = self;
        nativeBuildInputs = [
          rustToolchain
          rustPlatform.cargoSetupHook
          pkgs.autoPatchelfHook
        ];
        buildInputs = [
          cascLib
          pkgs.zlib
          pkgs.zlib.static
          pkgs.stdenv.cc.cc.lib
        ];
        cargoDeps = workspaceCargoDeps;
        CASCLIB_LIB_DIR = "${cascLib}/lib";
        ZLIB_STATIC_LIB_DIR = "${pkgs.zlib.static}/lib";
        buildPhase = ''
          runHook preBuild
          export HOME="$TMPDIR"
          cargo build --release --locked -p arreat-data
          runHook postBuild
        '';
        installPhase = ''
          runHook preInstall
          mkdir -p "$out/bin"
          install -m755 target/release/arreat-data "$out/bin/arreat-data"
          runHook postInstall
        '';
      };
    };
}
