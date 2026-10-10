{
  description = "Jabu - Cross-platform XMPP Client";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        
        # Development shell
        devShell = pkgs.mkShell {
          name = "jabu-dev";
          buildInputs = with pkgs; [
            # Core Rust toolchain
            rustc
            cargo
            rustup
            
            # Build tools
            pkg-config
            cmake
            protobuf
            
            # System dependencies for Dioxus/GTK/WebKit
            webkitgtk_4_1
            libayatana-appindicator
            xdotool
            librsvg
            openssl
            sqlite
            clang
            llvm
            
            # Development tools
            git
            curl
            wget
            jq
          ];
          
          RUST_BACKTRACE = "1";
          RUST_LOG = "debug";
          CARGO_HOME = "$HOME/.cargo";
          OPENSSL_DIR = "${pkgs.openssl.dev}/";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib/";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include/";
          PKG_CONFIG_PATH = "${pkgs.webkitgtk_4_1.dev}/lib/pkgconfig:${pkgs.openssl.dev}/lib/pkgconfig:${pkgs.libayatana-appindicator.dev}/lib/pkgconfig:${pkgs.librsvg.dev}/lib/pkgconfig";
          
          shellHook = ''
            # Add WASM target if not present
            if ! rustup target list --installed | grep -q "wasm32-unknown-unknown"; then
              echo "Installing WASM target..."
              rustup target add wasm32-unknown-unknown
            fi
            
            echo "🦀 Jabu Development Environment"
            echo "======================================"
            echo ""
            echo "Available commands:"
            echo "  dx serve --platform web       # Web dev server with hot reload"
            echo "  dx serve --platform desktop   # Desktop dev server with hot reload"
            echo "  dx bundle --platform web      # Production web build"
            echo "  dx bundle --platform desktop  # Production desktop build"
            echo "  cargo test --workspace        # Run all tests"
            echo "  cargo fmt --all               # Format code"
            echo "  cargo clippy --workspace      # Lint code"
            echo ""
            echo "Rust version: $(rustc --version)"
            echo "Cargo version: $(cargo --version)"
            echo ""
            echo "Targets installed:"
            rustup target list --installed | grep -E "(wasm32|aarch64|x86_64)" || echo "  Run: rustup target add wasm32-unknown-unknown"
            echo ""
          '';
        };
        
        # Docker build environment
        dockerEnv = pkgs.mkShell {
          name = "jabu-docker";
          buildInputs = with pkgs; [
            docker
            docker-compose
          ];
        };
        
      in {
        devShells.default = devShell;
        devShells.docker = dockerEnv;
        packages.default = pkgs.writeText "README" "Use 'nix develop' to enter the development shell";
      }
    );
}