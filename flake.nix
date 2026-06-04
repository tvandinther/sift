{
  description = "Hybrid full-text and semantic search over local text content";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Embedding model files for development and testing
    # Model: sentence-transformers/all-MiniLM-L6-v2
    # License: Apache-2.0
    model-config = {
      url = "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/config.json";
      flake = false;
    };
    model-tokenizer = {
      url = "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json";
      flake = false;
    };
    model-weights = {
      url = "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/model.safetensors";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, model-config, model-tokenizer, model-weights }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        commonBuildInputs = with pkgs; [
          pkg-config
          openssl
        ];

        nativeBuildInputs = commonBuildInputs;
        buildInputs = commonBuildInputs;

        # Create model directory with files from flake inputs
        # This provides the embedding model for tests and development
        modelDir = pkgs.runCommand "sift-model-cache" {} ''
          mkdir -p $out/sentence-transformers-all-MiniLM-L6-v2
          cp ${model-config} $out/sentence-transformers-all-MiniLM-L6-v2/config.json
          cp ${model-tokenizer} $out/sentence-transformers-all-MiniLM-L6-v2/tokenizer.json
          cp ${model-weights} $out/sentence-transformers-all-MiniLM-L6-v2/model.safetensors
        '';

      in
      {
        packages = {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "sift";
            version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;

            src = ./.;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            inherit nativeBuildInputs buildInputs;

            # Provide model cache for tests
            SIFT_MODEL_CACHE = "${modelDir}";

            # Tests can now run with cached model
            doCheck = true;

            meta = with pkgs.lib; {
              description = "Hybrid full-text and semantic search over local text content";
              homepage = "https://github.com/tvandinther/sift";
              license = licenses.mit;
              maintainers = [ "tvandinther" ];
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

          RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";

          # Provide cached embedding model for tests and development
          SIFT_MODEL_CACHE = "${modelDir}";

          shellHook = ''
            echo "🔍 sift development environment"
            echo "Rust version: $(rustc --version)"
            echo "Model cache: $SIFT_MODEL_CACHE"
            echo ""
            echo "Available commands:"
            echo "  cargo build          - Build the project"
            echo "  cargo test           - Run tests (with cached model)"
            echo "  cargo run -- <args>  - Run sift"
            echo ""
          '';
        };

        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/sift";
        };
      }
    );
}
