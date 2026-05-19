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
        darwinBuildInputs = with pkgs; lib.optionals stdenv.isDarwin [
          darwin.apple_sdk.frameworks.Security
          darwin.apple_sdk.frameworks.SystemConfiguration
          darwin.apple_sdk.frameworks.Foundation
          darwin.apple_sdk.frameworks.Metal
          darwin.apple_sdk.frameworks.MetalKit
          darwin.apple_sdk.frameworks.Accelerate
        ];

        nativeBuildInputs = commonBuildInputs;
        buildInputs = commonBuildInputs ++ darwinBuildInputs;

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
          ] ++ darwinBuildInputs;

          # Set up environment variables
          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

          # For Metal acceleration on macOS
          DYLD_FALLBACK_LIBRARY_PATH = pkgs.lib.optionalString pkgs.stdenv.isDarwin
            "${pkgs.darwin.apple_sdk.frameworks.Accelerate}/Library/Frameworks";

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
