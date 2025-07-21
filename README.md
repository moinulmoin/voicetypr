<div align="center">
  <img src="src-tauri/icons/icon.png" alt="VoiceTypr Logo" width="128" height="128">

  # VoiceTypr

  **Open Source voice to text tool, alternative to superwhisper, whispr flow**

  [![GitHub release](https://img.shields.io/github/v/release/moinulmoin/voicetypr)](https://github.com/moinulmoin/voicetypr/releases)
  [![License](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE.md)
  [![macOS](https://img.shields.io/badge/macOS-13.0+-black)](https://www.apple.com/macos)
  [![Downloads](https://img.shields.io/github/downloads/moinulmoin/voicetypr/total)](https://github.com/moinulmoin/voicetypr/releases)

  [Download](https://github.com/moinulmoin/voicetypr/releases/latest) • [Features](#features) • [Installation](#installation) • [Usage](#usage)
</div>

## 🎯 What is VoiceTypr?

VoiceTypr is open source ai voice to text dictation tool, alternative to Wispr Flow, SuperWisper for viber coders, super ai users. Pay once, User forever.

## ✨ Features

### 🎙️ **Instant Voice-to-Text**
- System-wide hotkey for quick recording
- Automatic text insertion at cursor position
- Works in any app - cursor, claude code, chatgpt, slack, etc

### 🤖 **Powered by local AI**
- 100% offline transcription - your voice never leaves your Mac
- Multiple model sizes for accuracy vs speed tradeoffs
- Support for 99+ languages out of the box

### 🚀 **Native Performance**
- Built with Rust and Tauri for blazing-fast performance
- Universal binary - optimized for both Intel and Apple Silicon
- Minimal resource usage with maximum efficiency

### 🔒 **Privacy First**
- Complete offline operation - no cloud, no tracking (only trial check)
- Your recordings stay on your device
- Open source for full transparency

### 🎨 **Clean Design**
- Clean, user interface
- Menubar integration for quick access
- Visual feedback during recording
- Auto-updates to keep you on the latest version

## 📦 Installation

### Requirements
- macOS 13.0 (Ventura) or later
- 3/4 GB free disk space (for AI models)
- Microphone access permission
- Accessibility access permission

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
4. **Start Transcribing** - Press your hot key anywhere to record

### Tips & Tricks

- 🎯 **Quick Cancel**: Double Press `Esc` while recording to cancel
- 📝 **Long Recordings**: VoiceTypr handles extended recordings seamlessly but shorter recordings are recommended to do.
- 🌍 **Multiple Languages**: Just speak - Whisper auto-detects the language
- ⚡ **Instant Insert**: Text appears right where your cursor is

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

## 📄 License

VoiceTypr is open source software licensed under the [GNU Affero General Public License v3.0](LICENSE.md).
</div>