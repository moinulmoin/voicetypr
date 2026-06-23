<div align="center">
  <img src="src-tauri/icons/icon.png" alt="Voicetypr Logo" width="128" height="128">

  # Voicetypr

  **Open Source AI Powered voice to text dictation tool, alternative to superwhisper, whispr flow**

  [![GitHub release](https://img.shields.io/github/v/release/moinulmoin/voicetypr)](https://github.com/moinulmoin/voicetypr/releases)
  [![License](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE.md)
  [![macOS](https://img.shields.io/badge/macOS-13.0+-black)](https://www.apple.com/macos)
  [![Windows](https://img.shields.io/badge/Windows-10%2F11-0078D6)](https://www.microsoft.com/windows)
  [![Downloads](https://img.shields.io/github/downloads/moinulmoin/voicetypr/total)](https://github.com/moinulmoin/voicetypr/releases)

  [Download](https://github.com/moinulmoin/voicetypr/releases/latest) • [Features](#features) • [Installation](#installation) • [Usage](#usage)
</div>

## 🎯 What is Voicetypr?

Voicetypr is an open source AI voice-to-text dictation tool, alternative to Wispr Flow and SuperWhisper. Available for macOS and Windows. Pay once, use forever.

## ✨ Features

### 🎙️ **Instant Voice-to-Text**
- System-wide hotkey for quick recording
- Automatic text insertion at cursor position
- Works in any app - cursor, claude code, chatgpt, slack, etc

### 🤖 **Powered by local AI**
- 100% offline transcription - your voice never leaves your device
- Multiple model sizes for accuracy vs speed tradeoffs
- Support for 99+ languages out of the box
- Hardware acceleration (Metal on macOS)

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
- GPU acceleration available (5-10x faster with NVIDIA, AMD, Intel GPUs)

### Quick Install

#### macOS
1. Download the latest [Voicetypr.dmg](https://github.com/moinulmoin/voicetypr/releases/latest)
2. Open the DMG and drag Voicetypr to Applications
3. Launch Voicetypr from Applications
4. Follow the onboarding to download your preferred AI model

> **Note**: Voicetypr is fully signed and notarized by Apple, so you can run it without security warnings.

#### Windows
1. Download the latest [Voicetypr installer](https://github.com/moinulmoin/voicetypr/releases/latest)
2. Run the installer
3. Launch Voicetypr from Start Menu
4. Follow the onboarding to download your preferred AI model

> **GPU Acceleration (5-10x faster)**
> - Voicetypr automatically uses your GPU if available
> - For best performance, ensure your graphics drivers are up to date:
>   - [NVIDIA Drivers](https://www.nvidia.com/drivers)
>   - [AMD Drivers](https://www.amd.com/support)
>   - [Intel Drivers](https://www.intel.com/content/www/us/en/support/products/80939/graphics.html)
> - Falls back to CPU automatically if GPU unavailable

## 🎮 Usage

### Getting Started

1. **Launch Voicetypr** - Find it in your Applications folder (macOS) or Start Menu (Windows)
2. **Grant Permissions** - Allow microphone access (and accessibility on macOS)
3. **Download a Model** - Choose from tiny to large models based on your needs
4. **Start Transcribing** - Press your hotkey anywhere to record

### Tips & Tricks

- 🎯 **Quick Cancel**: Double Press `Esc` while recording to cancel
- 📝 **Long Recordings**: Voicetypr handles extended recordings seamlessly but shorter recordings are recommended to do.
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

## 🔧 Troubleshooting

### Windows GPU Acceleration

Voicetypr automatically detects and uses your GPU for faster transcription. If you're experiencing slower performance:

**Update your graphics drivers** - This is the most common fix:
   - [NVIDIA Drivers](https://www.nvidia.com/drivers)
   - [AMD Drivers](https://www.amd.com/support)
   - [Intel Drivers](https://www.intel.com/content/www/us/en/support/products/80939/graphics.html)

> **Note**: Voicetypr always works - it automatically falls back to CPU if GPU acceleration is unavailable

## 📄 License

Voicetypr is licensed under the [GNU Affero General Public License v3.0](LICENSE.md).
</div>
