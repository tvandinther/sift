{
  description = "Hybrid full-text and semantic search over local text content";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        # Common build inputs for all platforms
        commonBuildInputs = with pkgs; [
          pkg-config
          openssl
        ];

        # Platform-specific build inputs
        # Darwin frameworks are now provided automatically by stdenv on macOS
        darwinBuildInputs = [ ];

        nativeBuildInputs = commonBuildInputs;
        buildInputs = commonBuildInputs;

      in
      {
        packages = {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "sift";
            version = "0.1.0";

            src = ./.;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            inherit nativeBuildInputs buildInputs;

            # Tests require network access for model downloads
            doCheck = false;

            meta = with pkgs.lib; {
              description = "Hybrid full-text and semantic search over local text content";
              homepage = "https://github.com/yourusername/sift";
              license = licenses.mit;
              maintainers = [ ];
              mainProgram = "sift";
            };
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            rust-analyzer
            cargo-watch
            cargo-edit

            # Build dependencies
            pkg-config
            openssl

            # Optional: for testing and development
            sqlite
          ];

          # Set up environment variables
          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

          shellHook = ''
            echo "🔍 sift development environment"
            echo "Rust version: $(rustc --version)"
            echo ""
            echo "Available commands:"
            echo "  cargo build          - Build the project"
            echo "  cargo test           - Run tests"
            echo "  cargo run -- <args>  - Run sift"
            echo ""
          '';
        };

        # Additional apps for easier invocation
        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/sift";
        };
      }
    );
}
