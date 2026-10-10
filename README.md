# Jabu

A modern, cross-platform XMPP client built with **Rust** and **Dioxus**.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![Dioxus](https://img.shields.io/badge/Dioxus-0.6-blue.svg)](https://dioxuslabs.com)

## Features

- 🔐 **End-to-End Encryption** - OMEMO (XEP-0384) support
- 💬 **Rich Messaging** - Text, media, reactions, replies, edits
- 👥 **Multi-User Chat** - Full MUC (XEP-0045) support
- 📝 **Microblogging** - PubSub-based blogging (Movim-style)
- 📱 **Cross-Platform** - Web, Desktop (Linux/macOS/Windows), Mobile (Android/iOS)
- 🔔 **Push Notifications** - Native on mobile, Web Push on browser
- 💾 **Local-First** - Works offline, syncs when online
- 🎨 **Modern UI** - TailwindCSS, responsive design
- 🌐 **Decentralized** - No central server, your data stays yours

## Supported XEPs

| XEP | Feature | Status |
|-----|---------|--------|
| RFC 6120 | XMPP Core | ✅ |
| RFC 6121 | XMPP IM | ✅ |
| XEP-0045 | Multi-User Chat | ✅ |
| XEP-0060 | Publish-Subscribe | ✅ |
| XEP-0163 | Personal Eventing Protocol | ✅ |
| XEP-0184 | Message Delivery Receipts | ✅ |
| XEP-0198 | Stream Management | ✅ |
| XEP-0280 | Message Carbons | 🚧 |
| XEP-0313 | Message Archive Management | ✅ |
| XEP-0357 | Push Notifications | 🚧 |
| XEP-0384 | OMEMO Encryption | ✅ |
| XEP-0393 | Styling | ✅ |
| XEP-0424 | Message Retraction | ✅ |
| XEP-0444 | Reactions | ✅ |

## Platforms

| Platform | Status | Notes |
|----------|--------|-------|
| Web (WASM) | ✅ | PWA with offline support |
| Linux | ✅ | AppImage, .deb, .rpm |
| macOS | ✅ | .dmg, notarized |
| Windows | ✅ | .msix, .exe |
| Android | 🚧 | APK/AAB |
| iOS | 🚧 | TestFlight |

## Getting Started

### Prerequisites

- Rust 1.75+
- Node.js 18+ (for TailwindCSS)
- dioxus-cli: `cargo install dioxus-cli`

### Building

```bash
# Clone the repository
git clone https://github.com/Danguilhen/Jabu
cd jabu

# Copy config template
cp config.toml.example config.toml
# Edit config.toml with your XMPP account

# Development (with hot reload)
dx serve --platform web
dx serve --platform desktop

# Build for production
dx bundle --platform web --release
dx bundle --platform desktop --release
dx bundle --platform mobile --release
```

### Mobile Development

```bash
# iOS
dx bundle --platform mobile --release
# Open ios/App.xcworkspace in Xcode

# Android
dx bundle --platform mobile --release
# Open android/ in Android Studio
```

## Architecture

```
jabu/
├── crates/
│   ├── xmpp-core/     # XMPP protocol implementation
│   ├── storage/       # Local storage abstraction
│   ├── ui/            # Dioxus components
│   ├── platform/      # Platform-specific code
│   └── app/           # Main application entry
├── assets/            # Icons, translations, manifests
└── config.toml        # Configuration
```

## Configuration

See `config.toml.example` for all options. Key settings:

```toml
[xmpp]
jid = "user@example.com"
password = "optional"  # Will prompt if not set
resource = "jabu"
```

## Development

### Project Structure

- **xmpp-core**: Wraps `prose-xmpp` for XMPP protocol handling
- **storage**: Abstracts local storage (redb/SQLite/IndexedDB)
- **ui**: Dioxus components with TailwindCSS
- **platform**: Platform-specific implementations
- **app**: Main entry point and CLI

### Running Tests

```bash
cargo test --workspace
```

### Code Style

```bash
cargo fmt --all
cargo clippy --workspace -- -D warnings
```

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests and lints
5. Submit a PR

## Acknowledgments

- [Movim](https://movim.eu) - Inspiration and XMPP expertise
- [prose-im](https://github.com/prose-im/prose-core-client) - XMPP library
- [Dioxus](https://dioxuslabs.com) - Amazing Rust UI framework
- [XMPP Standards Foundation](https://xmpp.org) - Protocol specifications