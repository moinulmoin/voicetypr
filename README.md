<div align="center">
  <img src="src-tauri/icons/icon.png" alt="VoiceTypr Logo" width="128" height="128">
  
  # VoiceTypr
  
  **Lightning-fast voice transcription for macOS**
  
  [![GitHub release](https://img.shields.io/github/v/release/moinulmoin/voicetypr)](https://github.com/moinulmoin/voicetypr/releases)
  [![License](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE.md)
  [![macOS](https://img.shields.io/badge/macOS-13.0+-black)](https://www.apple.com/macos)
  [![Downloads](https://img.shields.io/github/downloads/moinulmoin/voicetypr/total)](https://github.com/moinulmoin/voicetypr/releases)
  
  [Download](https://github.com/moinulmoin/voicetypr/releases/latest) • [Features](#features) • [Installation](#installation) • [Usage](#usage) • [Development](#development)
</div>

---

## 🎯 What is VoiceTypr?

VoiceTypr is a native macOS application that brings the power of OpenAI's Whisper AI directly to your desktop. With a simple keyboard shortcut, transform your voice into text instantly - no internet required, no subscriptions, just pure offline transcription magic.

<div align="center">
  <img src="docs/screenshots/hero-screenshot.png" alt="VoiceTypr Screenshot" width="600">
</div>

## ✨ Features

### 🎙️ **Instant Voice-to-Text**
- System-wide hotkey (`Cmd+Shift+Space`) for quick recording
- Automatic text insertion at cursor position
- Works in any app - emails, documents, chat, code editors

### 🤖 **Powered by Whisper AI**
- 100% offline transcription - your voice never leaves your Mac
- Multiple model sizes for accuracy vs speed tradeoffs
- Support for 99+ languages out of the box

### 🚀 **Native Performance**
- Built with Rust and Tauri for blazing-fast performance
- Universal binary - optimized for both Intel and Apple Silicon
- Minimal resource usage with maximum efficiency

### 🔒 **Privacy First**
- Complete offline operation - no cloud, no tracking
- Your recordings stay on your device
- Open source for full transparency

### 🎨 **Thoughtful Design**
- Clean, native macOS interface
- Menubar integration for quick access
- Visual feedback during recording
- Auto-updates to keep you on the latest version

## 📦 Installation

### Requirements
- macOS 13.0 (Ventura) or later
- 2GB free disk space (for AI models)
- Microphone access permission

### Quick Install

1. Download the latest [VoiceTypr.dmg](https://github.com/moinulmoin/voicetypr/releases/latest)
2. Open the DMG and drag VoiceTypr to Applications
3. Launch VoiceTypr from Applications
4. Follow the onboarding to download your preferred AI model

> **Note**: VoiceTypr is fully signed and notarized by Apple, so you can run it without security warnings.

## 🎮 Usage

### Getting Started

1. **Launch VoiceTypr** - Find it in your Applications folder
2. **Grant Permissions** - Allow microphone and accessibility access when prompted
3. **Download a Model** - Choose from tiny to large models based on your needs
4. **Start Transcribing** - Press `Cmd+Shift+Space` anywhere to record

### Model Selection Guide

| Model | Size | Speed | Accuracy | Best For |
|-------|------|-------|----------|----------|
| Tiny | 39 MB | ⚡⚡⚡⚡⚡ | ⭐⭐⭐ | Quick notes, fast transcription |
| Base | 74 MB | ⚡⚡⚡⚡ | ⭐⭐⭐⭐ | Balanced performance |
| Small | 244 MB | ⚡⚡⚡ | ⭐⭐⭐⭐ | General use |
| Medium | 769 MB | ⚡⚡ | ⭐⭐⭐⭐⭐ | Professional use |
| Large | 1.55 GB | ⚡ | ⭐⭐⭐⭐⭐ | Maximum accuracy |

### Tips & Tricks

- 🎯 **Quick Cancel**: Press `Esc` while recording to cancel
- 📝 **Long Recordings**: VoiceTypr handles extended recordings seamlessly
- 🌍 **Multiple Languages**: Just speak - Whisper auto-detects the language
- ⚡ **Instant Insert**: Text appears right where your cursor is

## 🛠️ Development

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Node.js](https://nodejs.org/) (v18+)
- [pnpm](https://pnpm.io/)
- Xcode Command Line Tools

### Setup

```bash
# Clone the repository
git clone https://github.com/moinulmoin/voicetypr.git
cd voicetypr

# Install dependencies
pnpm install

# Run in development mode
pnpm tauri dev
```

### Building

```bash
# Build for production
pnpm tauri build

# Build universal binary (Intel + Apple Silicon)
pnpm tauri build --target universal-apple-darwin
```

### Project Structure

```
voicetypr/
├── src/                    # React frontend
│   ├── components/         # UI components
│   ├── hooks/             # Custom React hooks
│   └── types/             # TypeScript types
├── src-tauri/             # Rust backend
│   ├── src/
│   │   ├── audio/         # Audio recording
│   │   ├── whisper/       # Whisper integration
│   │   └── commands/      # Tauri commands
│   └── capabilities/      # Security capabilities
├── scripts/               # Build and utility scripts
└── tests/                 # Test suites
```

### Testing

```bash
# Run all tests
pnpm test

# Frontend tests
pnpm test:ui

# Backend tests
cd src-tauri && cargo test
```

## 🤝 Contributing

We love contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Quick Start

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

VoiceTypr is open source software licensed under the [GNU Affero General Public License v3.0](LICENSE.md).

## 🙏 Acknowledgments

- [OpenAI Whisper](https://github.com/openai/whisper) for the incredible AI model
- [Tauri](https://tauri.app/) for the amazing app framework
- [whisper.cpp](https://github.com/ggerganov/whisper.cpp) for the efficient C++ implementation
- All our contributors and users!

## 📞 Support

- 🐛 **Bug Reports**: [GitHub Issues](https://github.com/moinulmoin/voicetypr/issues)
- 💡 **Feature Requests**: [GitHub Discussions](https://github.com/moinulmoin/voicetypr/discussions)
- 📧 **Email**: support@voicetypr.app

---

<div align="center">
  Made with ❤️ by <a href="https://github.com/moinulmoin">Moinul Moin</a>
  
  <br><br>
  
  <a href="https://www.producthunt.com/posts/voicetypr?utm_source=badge-featured&utm_medium=badge&utm_souce=badge-voicetypr" target="_blank">
    <img src="https://api.producthunt.com/widgets/embed-image/v1/featured.svg?post_id=XXXXX&theme=light" alt="VoiceTypr - Lightning-fast voice transcription for macOS | Product Hunt" width="250" height="54" />
  </a>
</div>