<div align="center">
  <img src="src-tauri/icons/icon.png" alt="VoiceTypr Logo" width="128" height="128">

  # VoiceTypr

  **Open Source voice to text tool, alternative to superwhisper, whispr flow**

  [![GitHub release](https://img.shields.io/github/v/release/moinulmoin/voicetypr)](https://github.com/moinulmoin/voicetypr/releases)
  [![License](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE.md)
  [![macOS](https://img.shields.io/badge/macOS-13.0+-black)](https://www.apple.com/macos)
  [![Windows](https://img.shields.io/badge/Windows-10%2F11-0078D6)](https://www.microsoft.com/windows)
  [![Downloads](https://img.shields.io/github/downloads/moinulmoin/voicetypr/total)](https://github.com/moinulmoin/voicetypr/releases)

  [Download](https://github.com/moinulmoin/voicetypr/releases/latest) • [Features](#features) • [Installation](#installation) • [Usage](#usage)
</div>

## 🎯 What is VoiceTypr?

VoiceTypr is an open source AI voice-to-text dictation tool, alternative to Wispr Flow and SuperWhisper. Available for macOS and Windows. Pay once, use forever.

## ✨ Features

### 🎙️ **Instant Voice-to-Text**
- System-wide hotkey for quick recording
- Automatic text insertion at cursor position
- Works in any app - cursor, claude code, chatgpt, slack, etc

### 🤖 **Powered by local AI**
- 100% offline transcription - your voice never leaves your device
- Multiple model sizes for accuracy vs speed tradeoffs
- Support for 99+ languages out of the box
- Hardware acceleration (Metal on macOS, CUDA on Windows)

### 🚀 **Native Performance**
- Built with Rust and Tauri for blazing-fast performance
- Optimized for each platform with hardware acceleration
- Minimal resource usage with maximum efficiency

### 🔒 **Privacy First**
- Complete offline operation - no cloud, no tracking (only trial check)
- Your recordings stay on your device
- Open source for full transparency

### 🤖 **AI Enhancement** (NEW)
- Transform your transcriptions with AI (Groq/Gemini)
- Smart presets: Prompts, Email, Commits, Notes
- Secure API key storage
- Requires internet connection for enhancement only

### 🎨 **Clean Design**
- Clean, user interface
- Menubar integration for quick access
- Visual feedback during recording
- Auto-updates to keep you on the latest version

## 📦 Installation

### Requirements

#### macOS
- macOS 13.0 (Ventura) or later
- 3-4 GB free disk space (for AI models)
- Microphone access permission
- Accessibility access permission

#### Windows
- Windows 10/11 (64-bit)
- 3-4 GB free disk space (for AI models)
- NVIDIA GPU recommended for CUDA acceleration (optional)

### Quick Install

#### macOS
1. Download the latest [VoiceTypr.dmg](https://github.com/moinulmoin/voicetypr/releases/latest)
2. Open the DMG and drag VoiceTypr to Applications
3. Launch VoiceTypr from Applications
4. Follow the onboarding to download your preferred AI model

> **Note**: VoiceTypr is fully signed and notarized by Apple, so you can run it without security warnings.

#### Windows
1. Download the latest [VoiceTypr installer](https://github.com/moinulmoin/voicetypr/releases/latest)
2. Run the installer
3. Launch VoiceTypr from Start Menu
4. Follow the onboarding to download your preferred AI model

> **Note**: CUDA acceleration will be automatically enabled if you have a compatible NVIDIA GPU.

## 🎮 Usage

### Getting Started

1. **Launch VoiceTypr** - Find it in your Applications folder (macOS) or Start Menu (Windows)
2. **Grant Permissions** - Allow microphone access (and accessibility on macOS)
3. **Download a Model** - Choose from tiny to large models based on your needs
4. **Start Transcribing** - Press your hotkey anywhere to record

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

VoiceTypr is licensed under the [GNU Affero General Public License v3.0](LICENSE.md).
</div>