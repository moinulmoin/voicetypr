# Microsoft Store Launch Playbook

Use this for every Microsoft Store submission/update. Keep direct GitHub installer releases separate from Store MSIX releases.

## Build source of truth

- Store artifact is built by `.github/workflows/store-msix.yml` or locally with:
  ```powershell
  pnpm run windows:store:msix
  ```
- Store manifest template: `src-tauri/msix/Package.appxmanifest`
- Store config: `src-tauri/tauri.windows.store.conf.json`
- Store package script: `scripts/build-msix-store.ps1`
- Direct Windows installer path remains separate: `scripts/release-windows.ps1`

## Partner Center package identity

These values must match the product identity shown in Partner Center:

- Package/Identity/Name: `IdeaplexaLLC.Voicetypr`
- Package/Identity/Publisher: `CN=98864543-9DFC-4D03-A121-2CA8406CB004`
- Package/Properties/PublisherDisplayName: `Ideaplexa LLC`
- Package Family Name: `IdeaplexaLLC.Voicetypr_jtk8shd9pzsk0`
- Store ID: `9P8J3X9B2JG6`

Do not change these unless Partner Center product identity changes.

## MSIX manifest requirements

- `Identity Version` in the template stays `0.0.0.0`; `scripts/build-msix-store.ps1` replaces it at staging time from `package.json`.
- `TargetDeviceFamily MinVersion` must remain a real Windows version. Current value: `10.0.19041.0`.
- Never let version replacement touch `TargetDeviceFamily MinVersion`; Partner Center rejects packages targeting `Windows MinVersion <= 10.0.17134.0`.
- `runFullTrust` is expected for Tauri desktop MSIX packages.
- `internetClient` is needed for licensing/update/service calls.
- `microphone` is needed for recording.

## Bundled runtime (Visual C++)

- The Store `voicetypr.exe` is built with the static CRT (`-C target-feature=+crt-static`), but `whisper-rs` enables OpenMP on Windows, which dynamically links `vcomp140.dll` (a Visual C++ Redistributable component with no static MSVC variant).
- `scripts/build-msix-store.ps1` bundles the VC++ runtime DLLs (CRT + OpenMP) app-local, next to `voicetypr.exe` in the package, sourced from the VS redist folder via `vswhere`. The build fails if `vcomp140.dll` is not staged.
- Because the dependency is integrated into the package, Store policy 10.2.4.1 (Software Dependencies) needs no description disclosure. Do not add VC++/runtime jargon to the public description.
- Unlike the direct NSIS installer (which ships `vc_redist.x64.exe`), the MSIX must be fully self-contained; never rely on a machine-wide redistributable.

## Store vs direct updater rules

- Store installs must not use the GitHub/Tauri updater.
- Store builds disable the updater in `src-tauri/tauri.windows.store.conf.json`.
- Runtime detection uses `get_distribution_info` and `is_store_install()`.
- Direct installer users keep GitHub updater behavior.
- When changing Windows behavior, check both distribution paths: Store MSIX and direct NSIS installer.

## Required validation before uploading

For any Store MSIX artifact:

1. Run the Store MSIX workflow or local Store build.
2. Confirm the package manifest inside the artifact has:
   ```xml
   <Identity ... Version="x.y.z.0" ... />
   <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.19041.0" ... />
   ```
   - `vcomp140.dll` and the VC++ CRT DLLs sit next to `voicetypr.exe` in the package (bundled app-local by the build script).
3. Confirm CI passes for frontend, macOS, and Windows.
4. Upload the `.msix` to Partner Center.
5. Partner Center warning for `runFullTrust` is expected; a hard package validation error is not.

## Restricted capability justification

Paste this when Partner Center asks why `runFullTrust` is needed:

```text
Voicetypr is a packaged desktop application built with Tauri. The app requires the runFullTrust capability to launch its Win32 desktop executable from the MSIX package.

Voicetypr provides offline voice typing and transcription for Windows. The desktop process is needed for microphone recording, system-wide global hotkeys, local audio processing, launching bundled helper binaries used by the app, and inserting completed transcription back into the user’s active application.

Voicetypr does not use runFullTrust to bypass user consent, access unrelated user data, or run hidden background services. The capability is required because the core product is a desktop utility that depends on native Windows desktop APIs and packaged helper executables.
```

## Partner Center product declarations

- Properties -> Product declarations: check "This product incorporates generative AI features, which utilize artificial intelligence to create new content - including text, images, audio, video, and code."
- Required because Voicetypr's optional text cleanup sends transcripts to LLM providers (OpenAI / Anthropic / Google Gemini) that generate new text. This satisfies Store policy 11.16 (Live Generative AI Content). Keep it checked while AI enhancement ships.

## Store listing copy rules

Write for customers, not developers.

Avoid in the public description:

- Vulkan
- sidecar
- CPU/GPU implementation details
- Whisper internals
- Tauri/MSIX/package details

Use technical terms only where customers expect them, such as optional feature bullets or support docs.

## Store description

```text
Voicetypr helps you type with your voice in the apps you already use.

Press a hotkey, speak naturally, and Voicetypr places the text where your cursor is. Use it in email, chat, notes, documents, browsers, writing tools, coding tools, and everyday text fields without switching to a separate editor.

Voicetypr is built for people who want to write faster, reduce typing, capture thoughts quickly, and stay focused. It works well for quick replies, long-form dictation, notes, prompts, emails, documents, and daily writing.

Dictation works offline by default, so your voice does not need to be sent to a cloud service for everyday transcription. You can also use optional text cleanup features when you want rough dictation turned into cleaner writing.

Voicetypr includes a free trial and a lifetime license option. No subscription is required.
```

## Product features

Add as separate feature rows:

```text
Type with your voice in any app
Works in email, chat, notes, documents, and browsers
Global hotkey recording
Push-to-talk mode
Toggle recording mode
Offline dictation by default
No cloud transcription required for everyday use
Private on-device voice transcription
Optional cleaner text for emails and writing
Audio and video file transcription
Searchable local transcript history
Copy and reuse previous transcripts
Choose speed or accuracy
Manage local transcription models
Works on Windows 10 and later
Designed for fast replies and long-form writing
Helpful for reducing typing strain
Useful for writing prompts, notes, and documents
No subscription required
Lifetime license option
```

## Keywords

Partner Center allows up to 7 keywords and no more than 21 total words across all keywords. Use this safe set:

```text
voice typing
dictation app
speech to text
offline dictation
transcription
hands free typing
productivity
```

Do not use `desktop voice recorder`; it attracts the wrong user intent.

## Other listing fields

- Copyright: `© 2026 Ideaplexa LLC. All rights reserved.`
- Additional license terms: leave blank unless legal terms change.
- Developed by: `Ideaplexa LLC`
- Category: Productivity
- Pricing: if using existing external licensing, Store price can be free; disclose trial/license in the description.
- Visibility: public and discoverable when ready for real launch.
- Markets: all worldwide markets unless there is a legal/support reason to restrict.

## Screenshots and assets

Required:

- At least 1 desktop screenshot.
- Recommended: 5-8 high-quality screenshots per Microsoft guidance.

Use screenshots that show real customer workflows:

1. Main app/settings screen.
2. Recording/pill UI.
3. Model/settings screen.
4. Text appearing in email/chat/docs/browser.
5. History or file transcription if ready.

Rules:

- PNG.
- At least 1366x768.
- No private data, keys, emails, customer names, or debug logs.
- Do not upload Xbox screenshots unless Xbox is actually supported.
- Trailers are optional; skip for first launch unless a polished demo exists.

## Microsoft docs

- Submission FAQ: https://learn.microsoft.com/en-us/windows/apps/publish/faq/submit-your-app
- Store listing info for MSIX apps: https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/add-and-edit-store-listing-info
