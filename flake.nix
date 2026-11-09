{
  description = "Jabu - Cross-platform XMPP Client";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        nixpkgsUnfree = import nixpkgs {
          inherit system;
          config = {
            allowUnfree = true;
            android_sdk.accept_license = true;
            allowUnfreePredicate = pkg: builtins.elem (builtins.getAttr "pname" pkg) [
              "android-sdk-cmdline-tools"
              "androidsdk"
              "android-sdk-platform-tools"
              "android-ndk-bundle"
              "android-sdk-platform-tools"
              "android-sdk-ndk"
            ];
          };
        };

        # Also set env var for good measure
        NIXPKGS_ALLOW_UNFREE = "1";

        pkgs = nixpkgsUnfree;

        # Use rustup-managed toolchain from user's home (matches rust-toolchain.toml)
        # We use nixpkgs rustup but run the pinned toolchain via rustup
        rustToolchain = pkgs.rust-bin or pkgs.rust;

        # Android SDK/NDK/JDK - use platform-tools, download NDK in shell hook
        androidSdk = nixpkgsUnfree.androidenv.androidPkgs.platform-tools;
        # NDK will be downloaded in shell hook if not present
        androidNdk = nixpkgsUnfree.androidenv.androidPkgs.ndk-bundle;
        jdk = nixpkgsUnfree.jdk21;
        gradle = nixpkgsUnfree.gradle;

        # Development shell
        devShell = pkgs.mkShell {
          name = "jabu-dev-2";
          buildInputs = with pkgs; [
            # rustup to run the pinned toolchain
            rustup
            unzip
            curl

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
            llvmPackages.libclang
            vulkan-loader
            vulkan-headers
            shaderc

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
            # Wayland screen capture: pipewiresrc ships inside pipewire, not in
            # gst-plugins-bad, so the package is needed for its plugin too.
            pipewire

            # Android SDK/NDK/JDK
            androidSdk
            androidNdk
            jdk
            gradle

            # Development tools
            git
            curl
            wget
            jq
            dioxus-cli
            tailwindcss

            # Local XMPP test fixture
            prosody
          ];

          RUST_BACKTRACE = "1";
          RUST_LOG = "debug";
          CARGO_HOME = "$HOME/.cargo";

          # Android environment
          ANDROID_HOME = "${androidSdk}";
          ANDROID_SDK_ROOT = "${androidSdk}";
          ANDROID_NDK_HOME = "${androidNdk}";
          ANDROID_NDK_ROOT = "${androidNdk}";
          JAVA_HOME = "${jdk}";
          NIXPKGS_ALLOW_UNFREE = "1";
          GRADLE_HOME = "${gradle}/share/gradle";
          PATH = "${gradle}/bin:${androidSdk}/bin:${androidNdk}/toolchains/llvm/prebuilt/linux-x86_64/bin:${jdk}/bin:${gradle}/bin:${pkgs.lib.makeBinPath [ jdk gradle androidSdk androidNdk ]}";

          # Linker settings for Android cross-compilation
          CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "${androidNdk}/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android21-clang";
          CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER = "${androidNdk}/toolchains/llvm/prebuilt/linux-x86_64/bin/armv7a-linux-androideabi21-clang";
          CARGO_TARGET_I686_LINUX_ANDROID_LINKER = "${androidNdk}/toolchains/llvm/prebuilt/linux-x86_64/bin/i686-linux-android21-clang";
          CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER = "${androidNdk}/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android21-clang";
          CARGO_TARGET_ARM_LINUX_ANDROIDEABI_LINKER = "${androidNdk}/toolchains/llvm/prebuilt/linux-x86_64/bin/armv7a-linux-androideabi21-clang";

          OPENSSL_DIR = "${pkgs.openssl.dev}/";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib/";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include/";
          PKG_CONFIG_PATH = "${pkgs.webkitgtk_4_1.dev}/lib/pkgconfig:${pkgs.openssl.dev}/lib/pkgconfig:${pkgs.libayatana-appindicator.dev}/lib/pkgconfig:${pkgs.librsvg.dev}/lib/pkgconfig:${pkgs.gst_all_1.gstreamer.dev}/lib/pkgconfig:${pkgs.gst_all_1.gst-plugins-base.dev}/lib/pkgconfig:${pkgs.gst_all_1.gst-plugins-bad.dev}/lib/pkgconfig:${pkgs.gst_all_1.gst-plugins-good.dev}/lib/pkgconfig:${pkgs.libnice.dev}/lib/pkgconfig:${pkgs.srtp.dev}/lib/pkgconfig:${pkgs.alsa-lib.dev}/lib/pkgconfig";

          shellHook = ''
            # Use the pinned rustup toolchain
            export RUSTUP_HOME="$HOME/.rustup"
            export CARGO_HOME="$HOME/.cargo"
            export PATH="$CARGO_HOME/bin:$PATH"

            # llama-cpp-sys uses bindgen, which needs libclang at build time.
            export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"

            # Wayland screen capture needs pipewiresrc, which pipewire ships
            # itself; gst-plugins-bad has no pipewire support in this build.
            export GST_PLUGIN_SYSTEM_PATH_1_0="${pkgs.pipewire}/lib/gstreamer-1.0''${GST_PLUGIN_SYSTEM_PATH_1_0:+:$GST_PLUGIN_SYSTEM_PATH_1_0}"

            # Android cross-compilation environment
            export ANDROID_NDK_HOME="$ANDROID_NDK_HOME"
            export CC_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android21-clang"
            export CXX_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android21-clang++"
            export AR_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
            export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android21-clang"
            export CC_armv7_linux_androideabi="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/armv7a-linux-androideabi21-clang"
            export CXX_armv7_linux_androideabi="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/armv7a-linux-androideabi21-clang++"
            export AR_armv7_linux_androideabi="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
            export CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/armv7a-linux-androideabi21-clang"
            export CC_i686_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/i686-linux-android21-clang"
            export CXX_i686_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/i686-linux-android21-clang++"
            export AR_i686_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
            export CARGO_TARGET_I686_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/i686-linux-android21-clang"
            export CC_x86_64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android21-clang"
            export CXX_x86_64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android21-clang++"
            export AR_x86_64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
            export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android21-clang"

            # Alias cargo/rustc to use the pinned toolchain (1.98.1)
            alias cargo="rustup run 1.98.1 cargo"
            alias rustc="rustup run 1.98.1 rustc"

            # Verify toolchain
            echo "Rust version: $(rustup run 1.98.1 rustc --version)"
            echo "Cargo version: $(rustup run 1.98.1 cargo --version)"

            # Android env verification
            echo ""
            echo "Android SDK: $ANDROID_HOME"
            echo "Android NDK: $ANDROID_NDK_HOME"
            echo "Java: $JAVA_HOME"
            echo "Gradle: $GRADLE_HOME"

            # Download full NDK with clang toolchains if not present
            if [ ! -d "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin" ]; then
              echo "Downloading Android NDK..."
              NDK_VER="r27c"
              NDK_URL="https://dl.google.com/android/repository/android-ndk-r27c-linux.zip"
              NDK_DIR="$HOME/.cache/android-ndk-r27c"
              if [ ! -d "$NDK_DIR" ]; then
                mkdir -p "$HOME/.cache"
                cd "$HOME/.cache"
                curl -L -o "android-ndk-r27c-linux.zip" "$NDK_URL"
                unzip -q "android-ndk-r27c-linux.zip"
                mv "android-ndk-r27c" "$NDK_DIR"
              fi
              export ANDROID_NDK_HOME="$NDK_DIR"
              echo "Using downloaded NDK at: $ANDROID_NDK_HOME"
            fi

            # Set up cross-compilation environment with downloaded NDK
            export CC_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android21-clang"
            export CXX_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android21-clang++"
            export AR_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
            export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android21-clang"
            export CC_armv7_linux_androideabi="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/armv7a-linux-androideabi21-clang"
            export CXX_armv7_linux_androideabi="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/armv7a-linux-androideabi21-clang++"
            export AR_armv7_linux_androideabi="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
            export CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/armv7a-linux-androideabi21-clang"
            export CC_i686_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/i686-linux-android21-clang"
            export CXX_i686_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/i686-linux-android21-clang++"
            export AR_i686_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
            export CARGO_TARGET_I686_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/i686-linux-android21-clang"
            export CC_x86_64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android21-clang"
            export CXX_x86_64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android21-clang++"
            export AR_x86_64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
            export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android21-clang"

            echo ""
            echo "🦀 Jabu Development Environment"
            echo "======================================"
            echo ""
            echo "Available commands:"
            echo "  dx serve --platform web       # Web dev server with hot reload"
            echo "  dx serve --platform desktop   # Desktop dev server with hot reload"
            echo "  dx bundle --platform web      # Production web build"
            echo "  dx bundle --platform desktop  # Production desktop build"
            echo "  dx build --platform android   # Android build"
            echo ""
            echo "Note: Using rustup toolchain 1.98.1 (pinned in rust-toolchain.toml)"
            echo "  cargo test --workspace        # Run all tests"
            echo "  cargo fmt --all               # Format code"
            echo "  cargo clippy --workspace      # Lint code"
            echo ""
            echo "Rust version: $(rustc --version)"
            echo "Cargo version: $(cargo --version)"
            echo ""
            echo "Targets installed:"
            rustup target list --installed | grep -E "wasm32|aarch64|armv7|i686|x86_64" || echo "  Run: rustup target add ..."
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
