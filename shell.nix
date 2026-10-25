{ pkgs ? import <nixpkgs> {} }:

let
  # Use stable Rust from nixpkgs with WASM target
  rustc = pkgs.rustc;
  cargo = pkgs.cargo;
  rustup = pkgs.rustup;
in
pkgs.mkShell {
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

    # Calls: GStreamer WebRTC media backend
    gst_all_1.gstreamer
    gst_all_1.gst-plugins-base
    gst_all_1.gst-plugins-good
    gst_all_1.gst-plugins-bad
    gst_all_1.gst-plugins-ugly
    gst_all_1.gst-libav
    libnice
    srtp
    alsa-lib
    pulseaudio
    # Wayland screen capture: pipewiresrc ships inside pipewire.
    pipewire
    
    # Development tools
    git
    curl
    wget
    jq
  ];
  
  # Environment variables
  RUST_BACKTRACE = "1";
  RUST_LOG = "debug";
  
  # Cargo configuration
  CARGO_HOME = "$HOME/.cargo";
  
  # OpenSSL for native TLS
  OPENSSL_DIR = "${pkgs.openssl.dev}/";
  OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib/";
  OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include/";
  PKG_CONFIG_PATH = "${pkgs.webkitgtk_4_1.dev}/lib/pkgconfig:${pkgs.openssl.dev}/lib/pkgconfig:${pkgs.libayatana-appindicator.dev}/lib/pkgconfig:${pkgs.librsvg.dev}/lib/pkgconfig:${pkgs.gst_all_1.gstreamer.dev}/lib/pkgconfig:${pkgs.gst_all_1.gst-plugins-base.dev}/lib/pkgconfig:${pkgs.gst_all_1.gst-plugins-bad.dev}/lib/pkgconfig:${pkgs.gst_all_1.gst-plugins-good.dev}/lib/pkgconfig:${pkgs.libnice.dev}/lib/pkgconfig:${pkgs.srtp.dev}/lib/pkgconfig:${pkgs.alsa-lib.dev}/lib/pkgconfig";
  
  # Shell hook - runs when entering the shell
  shellHook = ''
    # Wayland screen capture needs pipewiresrc, which pipewire ships itself;
    # gst-plugins-bad has no pipewire support in this build.
    export GST_PLUGIN_SYSTEM_PATH_1_0="${pkgs.pipewire}/lib/gstreamer-1.0''${GST_PLUGIN_SYSTEM_PATH_1_0:+:$GST_PLUGIN_SYSTEM_PATH_1_0}"

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
}