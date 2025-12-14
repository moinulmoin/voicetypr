# Changelog

# [1.11.0](https://github.com/moinulmoin/voicetypr/compare/v1.10.0...v1.11.0) (2025-12-14)


### Bug Fixes

* add debug output and explicit GITHUB_TOKEN export ([340ebc9](https://github.com/moinulmoin/voicetypr/commit/340ebc949a3bfa883055345e374e615842df2178))
* add skipChecks to bypass release-it token validation ([72e5e64](https://github.com/moinulmoin/voicetypr/commit/72e5e6462bc8db754c7dc238a0c60a2f25d15129))
* add toast window to capabilities and increase pill-toast gap ([138c902](https://github.com/moinulmoin/voicetypr/commit/138c902cd530140a0ea62f409231e1049c35a2d7))
* align OpenAI config flow ([#34](https://github.com/moinulmoin/voicetypr/issues/34)) ([80c2cb6](https://github.com/moinulmoin/voicetypr/commit/80c2cb6a981bf14dcda95a878446e84244f0db67))
* auto-set GITHUB_TOKEN from gh CLI for release-it ([9469a0e](https://github.com/moinulmoin/voicetypr/commit/9469a0e2142ac92652b6ffbc342b2f97b606928d))
* avoid double log extension ([820bb1e](https://github.com/moinulmoin/voicetypr/commit/820bb1e4ad5e1966caa3721849eae1508e301993))
* defer mic permission prompts until onboarding complete ([a51b379](https://github.com/moinulmoin/voicetypr/commit/a51b379d1c137d08cf641d3075674defdfc04246))
* ensure audio device listeners clean up ([51d5e3f](https://github.com/moinulmoin/voicetypr/commit/51d5e3f2d5510376a2ed32d85623e0ea9cfb1f60))
* improve AI config auto-selection ([311c61e](https://github.com/moinulmoin/voicetypr/commit/311c61eeb7c176ec42797aeb47b2209988e7b58e))
* improve OpenAI config UX with Update button and better switch visibility ([0a0fa64](https://github.com/moinulmoin/voicetypr/commit/0a0fa6401191d0f1de9c447df7e3750dacf51d61))
* improve recording feedback with sound, throttle, and duration gates ([a0627c1](https://github.com/moinulmoin/voicetypr/commit/a0627c199939d086e366e4fbd8c255a3fa5cad43))
* improve toast notification with dynamic width and better error messages ([04f90f1](https://github.com/moinulmoin/voicetypr/commit/04f90f18cb41e5e4827f660c84a7d11b06b6023b))
* license errors, app reset, and tauri config ([70238af](https://github.com/moinulmoin/voicetypr/commit/70238afa71865a12f3976f14bc4b6398431e7b60))
* redact API key logging ([e0ceb8d](https://github.com/moinulmoin/voicetypr/commit/e0ceb8d52cc72970396b78ca7c5e955847649e0d))
* remove unnecessary hooks and additional directories from AI config ([ade912b](https://github.com/moinulmoin/voicetypr/commit/ade912b57dbd8e245c646e220d0dff9f028874e3))
* route recording errors via pill toast ([3fba56f](https://github.com/moinulmoin/voicetypr/commit/3fba56f75103e1589998e615f94eece75af887a1))
* sync microphone list with system ([6de9eaa](https://github.com/moinulmoin/voicetypr/commit/6de9eaa7da0ed279cb49ea3ccd383b0f04820e61))
* treat only watchdog timeout as timeout ([edc260e](https://github.com/moinulmoin/voicetypr/commit/edc260e3efb1ba7dbc8bedd547a61f14bb638dff))
* Windows device ID fallback + license status timeout ([27ab86d](https://github.com/moinulmoin/voicetypr/commit/27ab86d9393a2a32a201f9db00b05bd942d9b6d3))


### Features

* add clipboard retention preference ([ba80d62](https://github.com/moinulmoin/voicetypr/commit/ba80d62ca92902861d9b412495c2f6938df1161f))
* improve AI model availability and auto-selection ([a44bdd9](https://github.com/moinulmoin/voicetypr/commit/a44bdd9b83a30d7239356e5a6ab277460fd98a76))
* move pill/toast lower and auto-reposition on monitor change ([39ab78b](https://github.com/moinulmoin/voicetypr/commit/39ab78b12d2300bf94a87da2e2670df30878350e))
* queue critical pill events for reliable delivery ([2799928](https://github.com/moinulmoin/voicetypr/commit/2799928b95287eb4e6257feb90bf56b5b51684b8))
* simplify pill UI to minimal 3-dot indicator with separate toast window ([defc907](https://github.com/moinulmoin/voicetypr/commit/defc90753145f00f2018968d67b15542bd6e13e8))

# [1.10.0](https://github.com/moinulmoin/voicetypr/compare/v1.9.0...v1.10.0) (2025-10-24)


### Bug Fixes

* **audio:** prevent empty or ultra-short recordings from causing errors; add mode-specific min durations (PTT≥1s, Toggle≥3s); auto-recover from Error→Idle and gate start transitions ([97e9fad](https://github.com/moinulmoin/voicetypr/commit/97e9fad0de58714cecf5f9a55f20cd17500bca78))
* avoid panic when requesting parakeet sidecar ([855a2ed](https://github.com/moinulmoin/voicetypr/commit/855a2ed9d2190eb1673a3cc166132fdacf612ff7))
* **macOS): bundle ffmpeg/ffprobe via platform config and resolve lookup in Contents/MacOS to prevent normalization failures in packaged app; chore(stt/soniox:** update async model to stt-async-v3 ([17108f6](https://github.com/moinulmoin/voicetypr/commit/17108f6dda9755ce52a8ac24ca28ddb3e9923e87))
* persist Parakeet model selection across restarts ([fffba52](https://github.com/moinulmoin/voicetypr/commit/fffba520d2ada9a9b489cc7c12d7cbdf570477f8))
* resolve Parakeet sidecar MLX module loading and build issues ([33c3913](https://github.com/moinulmoin/voicetypr/commit/33c39134e829363bb066e71a2dd006ff94928592))
* **tauri:** scope Parakeet sidecar to macOS builds and improve UX ([2f54a53](https://github.com/moinulmoin/voicetypr/commit/2f54a5307d5c0c93662d51fa00f06f448702246a))
* **tray:** gate selection until onboarding completes; add tests for tray label and selection logic; ui: simplify Upload section copy ([0ac219a](https://github.com/moinulmoin/voicetypr/commit/0ac219a0fcbe8e6f46c940a53dcc1cde2f103be7))


### Features

* :sparkles: update windows ffmpeg ([734fddb](https://github.com/moinulmoin/voicetypr/commit/734fddb335b627e7e66426c0ac50d7b4b705c2c4))
* add Parakeet MLX sidecar integration ([ead96e3](https://github.com/moinulmoin/voicetypr/commit/ead96e3f804bcba277f32767269d54aa42a9804e))
* **parakeet:** add version-aware v2/v3 download/load/delete with FluidAudio 0.6.1; fix false-success download for v2 by verifying loaded model and cache; autoload selected Parakeet model on startup ([bec2e3e](https://github.com/moinulmoin/voicetypr/commit/bec2e3e62bdc32c7ccad1af4bacc7280e4f53d4c))
* replace Python Parakeet with Swift/FluidAudio implementation ([10909df](https://github.com/moinulmoin/voicetypr/commit/10909df13b05074948333233307536c88ec3cad3))
* **tray:** unify model selection and keep tray/dashboard in sync ([748675e](https://github.com/moinulmoin/voicetypr/commit/748675e0a2df69b7521fdf3810f0da266c3fe04f))

# [1.9.0](https://github.com/moinulmoin/voicetypr/compare/v1.8.0...v1.9.0) (2025-10-12)


### Bug Fixes

* **ai:** restore Groq/Gemini enhancement path and keep OpenAI-compatible config; cache non-openai keys in backend ([b2664f8](https://github.com/moinulmoin/voicetypr/commit/b2664f8c355f5de165afbb1cc855f44fef7143de))


### Features

* **ai/openai): accept any OpenAI-compatible model; remove dead_code attr. refactor(ui,keyring): standardize Tauri args to snake_case for OpenAI test/save. chore(audio:** keep mutable bindings in normalizer for future ops. ([5167f0c](https://github.com/moinulmoin/voicetypr/commit/5167f0c3f2f738f4689a30907b3dce053532bba3))
* **ai:** add OpenAI-compatible provider and endpoint normalization; fix pill feedback timing; cleanups ([a28030b](https://github.com/moinulmoin/voicetypr/commit/a28030bcb4fb2ed7a3f5c48f7fd940cbd5edee13))
* **audio:** normalize all inputs to 16k mono s16 + 5s gate & reduce ([937bbb3](https://github.com/moinulmoin/voicetypr/commit/937bbb3620ce218967ac63b64e456d683fcf4295))
* **help:** include hashed Device ID in ‘Copy System Info’ via new get_device_id Tauri command; hide mic/accessibility permission lines on Windows to avoid confusion ([ac80b7b](https://github.com/moinulmoin/voicetypr/commit/ac80b7b686d4e8a0ed306a3f9c8ca408f9610d45))
* **onboarding:** add centered model legend (Speed, Accuracy, Size, Recommended) before model list to match dashboard ([3257614](https://github.com/moinulmoin/voicetypr/commit/3257614f0a5beab163f9d39e8380b28dbde8d985))
* **tray:** add Recent Transcriptions copy, Recording Mode submenu, and Check for Updates; sync Dashboard↔Tray mode changes; refresh tray on history changes ([7accfdb](https://github.com/moinulmoin/voicetypr/commit/7accfdbc2f205abca95677654239f5031d0d21b6))

# [1.8.0](https://github.com/moinulmoin/voicetypr/compare/v1.7.0...v1.8.0) (2025-09-20)


### Bug Fixes

* Add support for multi-channel audio recording (4+ channels) ([54e2a0e](https://github.com/moinulmoin/voicetypr/commit/54e2a0e61b53e61fd0699ab77573a7494928e187))
* Enable macOS Cmd+Ctrl hotkey combinations ([e427cae](https://github.com/moinulmoin/voicetypr/commit/e427caed5432a4d31a12866d29dc665d13b83f3a))
* Extend offline grace period to 90 days for Issue [#15](https://github.com/moinulmoin/voicetypr/issues/15) ([7f7bef8](https://github.com/moinulmoin/voicetypr/commit/7f7bef83a8b0ff35108aa0a7f446c553398c24bc))
* Implement secure licensing with grace periods ([b123c36](https://github.com/moinulmoin/voicetypr/commit/b123c36e778b4010a20f29338a915a52edc68796))
* Improve hotkey system and shorten feedback messages ([bc5ee35](https://github.com/moinulmoin/voicetypr/commit/bc5ee356fe3951d197f0c84e19afe2214f3834e9))
* Improve language selection UX with alphabetical sorting and name-based search ([5f2d631](https://github.com/moinulmoin/voicetypr/commit/5f2d631d0a5562fb82c31db0c0a9a4c9a3137d35))
* Reduce drag-and-drop area height for better UI balance ([5a3dd8c](https://github.com/moinulmoin/voicetypr/commit/5a3dd8c320c4bfb7d46b7be5b1a486e340ca6d0c))
* Remove redundant audio validation to solve Issue [#16](https://github.com/moinulmoin/voicetypr/issues/16) "Audio too quiet" ([b977f7b](https://github.com/moinulmoin/voicetypr/commit/b977f7bde0182cad6d262ec4fb53c16083ca22a6))
* resolve UnwindSafe trait issues in panic prevention tests ([f431ee1](https://github.com/moinulmoin/voicetypr/commit/f431ee1a1024cf349daf8aa552021e0da7104e4a))
* Restore download progress updates for model downloads ([e3c13b8](https://github.com/moinulmoin/voicetypr/commit/e3c13b8e54ac34453bdc43d267535f449997f923))


### Features

* Add audio file upload and transcription support ([beed3d0](https://github.com/moinulmoin/voicetypr/commit/beed3d0d8435ba2c1cb8c3bf50ddde7459ec8009))
* Add audio file upload for transcription with multi-format support ([38f0c58](https://github.com/moinulmoin/voicetypr/commit/38f0c583b20ce090a5ce5ff40ba6579171d419a0))
* add export to JSON functionality with backend implementation ([7a18ce1](https://github.com/moinulmoin/voicetypr/commit/7a18ce140f2f42d9fa59498ed21bcc34d1a196bf))
* add Help section and share stats functionality ([e67dc68](https://github.com/moinulmoin/voicetypr/commit/e67dc683e3d8dfc67bde3ad5dc8a7af4f662b137))
* add microphone selection with bidirectional sync ([1791519](https://github.com/moinulmoin/voicetypr/commit/179151968315c9bb5bd322026f43a0a4fd0e11df))
* Add push-to-talk recording mode ([#13](https://github.com/moinulmoin/voicetypr/issues/13)) ([a6ef9ae](https://github.com/moinulmoin/voicetypr/commit/a6ef9aeaedd27ebbe02a9619a14da99e1170c967))
* Comprehensive hotkey system improvements with physical key support ([6938472](https://github.com/moinulmoin/voicetypr/commit/693847238c7dc8cfc4c471653a904791cf746e62))
* enhance history page with search and improved UI ([8bd2135](https://github.com/moinulmoin/voicetypr/commit/8bd2135dc727847254557ca1d1c499b8168fffde))
* Enhance HotkeyInput with editing state notification and update onboarding hotkey initialization ([43dca35](https://github.com/moinulmoin/voicetypr/commit/43dca35176afcd99592b4323a0e267c0b8971964))
* Enhanced license activation with OS metadata for better device identification ([c7b52da](https://github.com/moinulmoin/voicetypr/commit/c7b52da74e73582f45924474996dcb0b155ffba9))
* Improve history card display ([d1a0e67](https://github.com/moinulmoin/voicetypr/commit/d1a0e67983b88a404a9dacf16bb6b46343cb039b))
* improve UI/UX for support links and Quick Tips section ([c3f818f](https://github.com/moinulmoin/voicetypr/commit/c3f818f8b910f51e38075b8bf64d144c7be8a804))
* optimize transcription updates with append-only system ([5852078](https://github.com/moinulmoin/voicetypr/commit/5852078621ef67687a6b5b9f4c993760f34e5c4a))
* redesign Advanced section UI/UX ([ae5f1ca](https://github.com/moinulmoin/voicetypr/commit/ae5f1ca8a141fbcb81a047db42adfcdf3765351c))
* redesign Formatting page (formerly Enhancements) with improved UI/UX ([e006317](https://github.com/moinulmoin/voicetypr/commit/e006317a25a6f055a1949e82cbbea4ff0f1c857d))
* redesign Models page with improved UI/UX ([53f8837](https://github.com/moinulmoin/voicetypr/commit/53f88379a82ec790b1af0968f0c620d2bc601317))
* redesign overview dashboard with streak tracking and interactive stats ([415d730](https://github.com/moinulmoin/voicetypr/commit/415d730ca388219cef9bb5746aebd8da2054131f))
* redesign Settings page with modern UI ([2d77331](https://github.com/moinulmoin/voicetypr/commit/2d773311e9289e9a11c17f14836006c3fbbac40e))
* split Account section into License and About sections ([4a06489](https://github.com/moinulmoin/voicetypr/commit/4a06489a1873761f03515f7ee37d9aa3fcfd3b6e))


### Performance Improvements

* implement caching layer for settings and license to reduce I/O overhead ([391d752](https://github.com/moinulmoin/voicetypr/commit/391d752185f67eeb2f976011a7f6e3ffaee4f1a6))
* optimize transcription flow by reducing delays and improving UX ([22e303a](https://github.com/moinulmoin/voicetypr/commit/22e303aca2e98b3053f6b295c02de1bb5c50cccf))

# [1.7.0](https://github.com/moinulmoin/voicetypr/compare/v1.6.2...v1.7.0) (2025-08-20)


### Bug Fixes

* handle misheard filler words in base enhancement prompt ([b2db125](https://github.com/moinulmoin/voicetypr/commit/b2db125527508fc40855fe360420eb5b2bc06873))
* hotkey display platform-specific symbols ([04dde4b](https://github.com/moinulmoin/voicetypr/commit/04dde4bea3b3bbcb0e5097f3cac9064aae6693d8))
* make enhancement prompt comprehensive for all error types ([73a1612](https://github.com/moinulmoin/voicetypr/commit/73a16128d8ecc3df9aff6a1ac7a7eb134f8b4dfb))
* resolve menu bar updates and AI enhancement issues ([6f3b15f](https://github.com/moinulmoin/voicetypr/commit/6f3b15f905bc4a90d3a750846f755aaa497bfbdd))
* restore license expiration dashboard navigation on hotkey press ([0cdbfef](https://github.com/moinulmoin/voicetypr/commit/0cdbfef6801f2a5c3483a22c6941877a432b921d))
* update enhancement mode descriptions for accuracy ([29d2679](https://github.com/moinulmoin/voicetypr/commit/29d2679abe2406d978d21d43889ff9a68d44cd3e))


### Features

* improve AI enhancement settings discoverability ([84dac26](https://github.com/moinulmoin/voicetypr/commit/84dac2654f0d2f6fa444290d197c04c90dbc0c30))

## [1.6.2](https://github.com/moinulmoin/voicetypr/compare/v1.6.0...v1.6.2) (2025-08-14)


### Bug Fixes

* enable runtime hotkey updates without app restart ([f2701f8](https://github.com/moinulmoin/voicetypr/commit/f2701f85f84a2d1d45b81cc0d3fe7b2bc7fe92eb))


### Features

* :sparkles: add audio validation and enhanced error logging system ([d9ecc06](https://github.com/moinulmoin/voicetypr/commit/d9ecc06d870e807f50a4c9fa0f1925ae9e299e24))
* enhance logging system with comprehensive diagnostics and monitoring ([64fb1c2](https://github.com/moinulmoin/voicetypr/commit/64fb1c21a31a4ebdf6f39595542966b23b1e0014))
* implement comprehensive production-ready logging system ([5c8bee1](https://github.com/moinulmoin/voicetypr/commit/5c8bee1ebbb75c69643b76673fd9bfea41ed9775))
* implement smart Windows installer with automatic GPU detection and fallback ([f314952](https://github.com/moinulmoin/voicetypr/commit/f314952df72fc7948333021e2d48799f82319d44))


### BREAKING CHANGES

* Removed apply_pending_shortcut command (was internal only)

## [1.6.1](https://github.com/moinulmoin/voicetypr/compare/v1.6.0...v1.6.1) (2025-08-14)


### Features

* :sparkles: add audio validation and enhanced error logging system ([d9ecc06](https://github.com/moinulmoin/voicetypr/commit/d9ecc06d870e807f50a4c9fa0f1925ae9e299e24))
* enhance logging system with comprehensive diagnostics and monitoring ([64fb1c2](https://github.com/moinulmoin/voicetypr/commit/64fb1c21a31a4ebdf6f39595542966b23b1e0014))
* implement comprehensive production-ready logging system ([5c8bee1](https://github.com/moinulmoin/voicetypr/commit/5c8bee1ebbb75c69643b76673fd9bfea41ed9775))
* implement smart Windows installer with automatic GPU detection and fallback ([f314952](https://github.com/moinulmoin/voicetypr/commit/f314952df72fc7948333021e2d48799f82319d44))

# [1.6.0](https://github.com/moinulmoin/voicetypr/compare/v1.5.0...v1.6.0) (2025-08-08)


### Features

* :sparkles: add Windows dual build system with CPU and Vulkan GPU variants ([302d2f0](https://github.com/moinulmoin/voicetypr/commit/302d2f0e39aeee2af10d1f67738bb34b87b5e8fc))
* :sparkles: move vulkan feature flag to windows-specific dependency for whisper-rs ([f3dae0b](https://github.com/moinulmoin/voicetypr/commit/f3dae0b83dfa5243e29131cb7f429113712291be))
* :sparkles: set small English model as recommended for improved language support ([f95087f](https://github.com/moinulmoin/voicetypr/commit/f95087fc5e49d7ed5c35b894911f06ef4f6886ab))
* add GPU-aware smart installer with auto-detection and update system ([3319d1e](https://github.com/moinulmoin/voicetypr/commit/3319d1ed4a512ca8487a2eeb812d92ccf7c7bcee))
* add smart GPU detection and installer for Windows with Vulkan support ([81ba6ca](https://github.com/moinulmoin/voicetypr/commit/81ba6cac1a6b7ebcdd59efe07f11265ec3060400))
* add Windows dual build release script with CPU and GPU variants ([ad05880](https://github.com/moinulmoin/voicetypr/commit/ad05880d08a58ef089c9742aee651fa277449e37))
* add Windows GPU support with Vulkan installer and release scripts ([6d85863](https://github.com/moinulmoin/voicetypr/commit/6d858635bd6e555c93fa92d410e84efb71fab8a5))
* add Windows release script with MSI installer and update artifacts ([ed55678](https://github.com/moinulmoin/voicetypr/commit/ed5567866034824cdf960da93cd37a5bc9e58b30))
* implement robust Windows GPU dual-build support ([0aa71b6](https://github.com/moinulmoin/voicetypr/commit/0aa71b6863dd92a74ee9943fce7c01132bd135c6))


### BREAKING CHANGES

* Windows now has separate CPU and GPU builds

Changes:
- Add Vulkan GPU support for Windows (works with NVIDIA, AMD, Intel)
- Implement macOS Metal fallback to CPU on initialization failure
- Add VULKAN_SDK pre-flight check in Windows release script
- Enhance README with GPU requirements and troubleshooting
- Improve runtime logging to show active backend

Critical fixes after review:
- Fix macOS Metal early return to ensure consistent logging
- Remove impossible conditional compilation branches
- Track actual GPU usage for accurate backend reporting

The CPU build ensures universal Windows compatibility while the GPU
build with Vulkan provides ~2-3x faster transcription for users with
compatible GPUs.

# [1.5.0](https://github.com/moinulmoin/voicetypr/compare/v1.4.0...v1.5.0) (2025-08-06)


### Bug Fixes

* :bug: simplify Windows window flag application by removing redundant error handling ([2c04f14](https://github.com/moinulmoin/voicetypr/commit/2c04f14478ab142f65f52d706463515e5fe4c1fb))


### Features

* :sparkles: add timeout to recorder thread, improve window handling, and enhance security validations ([758231d](https://github.com/moinulmoin/voicetypr/commit/758231dc3cb035d7a0517231075211a56a851d10))
* :sparkles: refactor: optimize platform detection with static boolean values ([204e4f3](https://github.com/moinulmoin/voicetypr/commit/204e4f35edffd8291f81f68e4d0ffa46cc855d73))
* add advanced settings section with permissions, reset options and diagnostics ([56bcc9b](https://github.com/moinulmoin/voicetypr/commit/56bcc9b03c5c7df1d4b618597f8438b65536c2bf))
* add license management and log cleanup functionality ([5834ce6](https://github.com/moinulmoin/voicetypr/commit/5834ce69e0dca69839651815316ac6f21f6e774f))
* add platform-specific settings and advanced configuration UI ([18d5e46](https://github.com/moinulmoin/voicetypr/commit/18d5e46b123c70454d36a7274c4384f578f20221))
* add Windows release scripts and configuration files for cross-platform builds ([e1516de](https://github.com/moinulmoin/voicetypr/commit/e1516ded527c60ed7933a5c2bf8677e0fe212e94))
* refactor the app with cross-platform permissions and settings UI ([930f9a2](https://github.com/moinulmoin/voicetypr/commit/930f9a23ce6cd8efcf59a762246b7dd0a82ac09c))

# [1.4.0](https://github.com/moinulmoin/voicetypr/compare/v1.3.0...v1.4.0) (2025-08-01)


### Bug Fixes

* :bug: test and remove sentry ([57be5a3](https://github.com/moinulmoin/voicetypr/commit/57be5a38df7d6ba86a371158312c5585ee39d3da))


### Features

* :sparkles: add AI enhancement integration with API key management and model selection ([994f21b](https://github.com/moinulmoin/voicetypr/commit/994f21b747167cfad5a1fbd080ba423efecdf027))
* :sparkles: add Gemini provider integration and related tests ([1263503](https://github.com/moinulmoin/voicetypr/commit/1263503745966d0ee7ebca17a31c949274dffbb0))
* :sparkles: added sentry well ([2f1ceb5](https://github.com/moinulmoin/voicetypr/commit/2f1ceb5d3d7fe45aa79e1878af4764b1f8b959ba))
* :sparkles: fixes ([682c267](https://github.com/moinulmoin/voicetypr/commit/682c26775badf7ac6fa691c6ea420a0b382a1dc2))
* :sparkles: implement AI enhancement options and settings management ([843f830](https://github.com/moinulmoin/voicetypr/commit/843f8304da1146ed2028d51c17f05814f9140ac6))
* :sparkles: implement better key storing ([e6a5344](https://github.com/moinulmoin/voicetypr/commit/e6a5344bde1662cab0540b20d5eaf8013e4e1603))
* :sparkles: improve checker state ([876fda7](https://github.com/moinulmoin/voicetypr/commit/876fda73688a8b632f944b202903e627e895d01d))
* :sparkles: improve prompts and ui ([5cd995d](https://github.com/moinulmoin/voicetypr/commit/5cd995d0c2c0a5361b71e85f6d71c6d7f21b2ced))
* :sparkles: investigating audio issue ([fd70cc9](https://github.com/moinulmoin/voicetypr/commit/fd70cc9d0f9ba8e905193bbbdde898cc28997bb8))
* :sparkles: little fix and improvements ([d481603](https://github.com/moinulmoin/voicetypr/commit/d48160335a448295db699abd39118068098bd507))
* :sparkles: refactor audio processing and permission handling ([bdc7f35](https://github.com/moinulmoin/voicetypr/commit/bdc7f35a4c403400ae67862453528dabf84acddb))
* :sparkles: remove debug logging and add proper resampling ([1b415a2](https://github.com/moinulmoin/voicetypr/commit/1b415a2ed04a825170549d6c630d595691facc26))
* :sparkles: remove deprecated Sentry test commands and related documentation ([5aab0a7](https://github.com/moinulmoin/voicetypr/commit/5aab0a75443f45ff801a0bd5bb238ecc39e44229))
* :sparkles: update ai enchance and general settings ([e844915](https://github.com/moinulmoin/voicetypr/commit/e844915a3e2abec9b9983bf25fd58a06d53b66de))

# [1.3.0](https://github.com/moinulmoin/voicetypr/compare/v1.2.2...v1.3.0) (2025-07-22)


### Features

* :sparkles: add automatic update checks and tray menu functionality, suppress blank and non speech voice ([c812355](https://github.com/moinulmoin/voicetypr/commit/c81235554cc99d282965a89ebf0f5e4510821024))
* :sparkles: add huge perf improvement and improved audio level meter and silence detection ([aa2601a](https://github.com/moinulmoin/voicetypr/commit/aa2601a279d27646812609f43a7f0811c767c514))
* :sparkles: add new models, keep only 2 permission, add reset app data ([3d3dd56](https://github.com/moinulmoin/voicetypr/commit/3d3dd5613588f9512202316ad6eee06b8ecdc7ed))
* :sparkles: add sentry, fix esc not stopping, add pill tooltip feedback ([82c88b1](https://github.com/moinulmoin/voicetypr/commit/82c88b151a377cb0ce0b520cc6363aa1b9db8781))
* :sparkles: add translation, make download and read seperate action ([3e9e61a](https://github.com/moinulmoin/voicetypr/commit/3e9e61a294f6c0363da92ba64682f2226471f0d3))
* :sparkles: enable GPU acceleration and multi-threading for improved transcription performance ([1f3c1b3](https://github.com/moinulmoin/voicetypr/commit/1f3c1b39e570790b4f3424bed4be4e2d77db63c8))

## [1.2.2](https://github.com/moinulmoin/voicetypr/compare/v1.2.1...v1.2.2) (2025-07-20)


### Features

* :sparkles: fix download updates ([6d72f2d](https://github.com/moinulmoin/voicetypr/commit/6d72f2d33c650d282e5b701fb49f01f1cacd4b79))

## [1.2.1](https://github.com/moinulmoin/voicetypr/compare/v1.2.0...v1.2.1) (2025-07-20)

# [1.2.0](https://github.com/moinulmoin/voicetypr/compare/v1.1.1...v1.2.0) (2025-07-20)


### Features

* :sparkles: enhance permissions management in OnboardingDesktop, add automation permission checks, and update sidebar to include advanced section ([ca56910](https://github.com/moinulmoin/voicetypr/commit/ca56910ceec1b686a3fa76ee725017de22a9b52c))

## [1.1.1](https://github.com/moinulmoin/voicetypr/compare/v1.1.0...v1.1.1) (2025-07-19)


### Features

* :sparkles: add tauri-plugin-macos-permissions-api dependency, enhance model management in App component, and improve accessibility permission handling ([fd96e05](https://github.com/moinulmoin/voicetypr/commit/fd96e05853fb35794eac1565fc5670855ba0705c))
* :sparkles: fix external link handling in AboutSection,  add updater capabilities in default.json ([d627ff5](https://github.com/moinulmoin/voicetypr/commit/d627ff5194257d9f1ad91320c51c9f7849fceaac))
* :sparkles: refactor model management integration in App and OnboardingDesktop components, enhance loading state handling, and improve model status response structure in Tauri commands ([82fd144](https://github.com/moinulmoin/voicetypr/commit/82fd144ee717e79315a7957367878ba2a0498055))
* :sparkles: remove modelManagement prop from OnboardingDesktop, update useModelManagement hook for onboarding context, and adjust event handling for model downloads ([d0e079c](https://github.com/moinulmoin/voicetypr/commit/d0e079ca154b2f6ce41152856aeb7b4828c8e3bd))
* :sparkles: show loading while verifying downloads ([1197069](https://github.com/moinulmoin/voicetypr/commit/11970698c976c6728025809633e9ebf775ba8675))
* :sparkles: streamline model download handling in Tauri commands, enhance logging for download progress, and simplify event emissions in useModelManagement hook ([462ad1d](https://github.com/moinulmoin/voicetypr/commit/462ad1d53658cbe789a1f396ce3aaae2d51f8310))

# [1.1.0](https://github.com/moinulmoin/voicetypr/compare/v1.0.0...v1.1.0) (2025-07-18)


### Features

* :sparkles: fix license cache ([435f8a5](https://github.com/moinulmoin/voicetypr/commit/435f8a557962fb845302d20dd5aac57acbe9a26f))
* :sparkles: remove CI, release, and test workflows from GitHub Actions for project restructuring ([c763ae0](https://github.com/moinulmoin/voicetypr/commit/c763ae083c63f2982edeb066d43b0ddc0e87881e))
* :sparkles: reorganize imports in App component, update active section state, enhance AboutSection with app version retrieval, and clear license cache on startup for fresh checks ([762e158](https://github.com/moinulmoin/voicetypr/commit/762e1583b2aa6eac6b4ceb5736e406671a0e5318))
* :sparkles: replace Twitter icon with XformerlyTwitter in AboutSection and reorganize imports for better structure ([97c05c6](https://github.com/moinulmoin/voicetypr/commit/97c05c6f4515ef29a1b7bf4cbe05e7991bb3116d))
* :sparkles: update .gitignore to include release files, clean up AboutSection and LicenseContext for improved readability, and fix URL in license commands ([1f9d501](https://github.com/moinulmoin/voicetypr/commit/1f9d501b5e56a7f5a8d66251d17e420ca2b246f4))
* :sparkles: update script ([ff5ebeb](https://github.com/moinulmoin/voicetypr/commit/ff5ebeb4816325efaaa2cc74e4a08801bade61c9))

# 1.0.0 (2025-07-18)


### Bug Fixes

* :bug: pill showing ([b67c1d1](https://github.com/moinulmoin/voicetypr/commit/b67c1d1cd926f67a132637f6bf870815cae19b10))
* :bug: silence audio checking after recording ([4de6aa1](https://github.com/moinulmoin/voicetypr/commit/4de6aa14bc346b487930d8bfdaa87eda40dfc603))
* :bug: sync with recent history ([0ccecec](https://github.com/moinulmoin/voicetypr/commit/0ccececb2bb23a1a45ea894e216540c0f1b34e4a))


### Features

* :sparkles: add compact recording status setting, enhance feedback messages in RecordingPill, and implement cancellation handling in transcription process ([f3b42ff](https://github.com/moinulmoin/voicetypr/commit/f3b42ff3571efceb2d6e1ff9d27c5d899cfc7f72))
* :sparkles: add dialog plugin support and enhance model management with improved UI interactions and state handling ([9b7d32e](https://github.com/moinulmoin/voicetypr/commit/9b7d32eea4619fd6bee128470c277888234ea477))
* :sparkles: add formatting script to package.json and refine model management with updated model descriptions and UI enhancements ([2c2a997](https://github.com/moinulmoin/voicetypr/commit/2c2a9978f776fe71cb781b542242de511e6fa583))
* :sparkles: add iOS spin animation, refactor ModelCard component, and update RecordingPill to use IOSSpinner ([f30422e](https://github.com/moinulmoin/voicetypr/commit/f30422e37244282a55c4932b5f9ef54bf30d6535))
* :sparkles: add new dependencies and update configuration for VoiceTypr ([34d30c9](https://github.com/moinulmoin/voicetypr/commit/34d30c99a91731d76fa2021bc383cf26aec7bbc4))
* :sparkles: adjust compact mode styles in RecordingPill for better visual consistency and update audio wave animation scaling ([fb7e4c7](https://github.com/moinulmoin/voicetypr/commit/fb7e4c71f42a8ec39011f1395ade4126da2107bb))
* :sparkles: adjust RecordingPill layout for better feedback message display and update window size calculations for improved visibility ([22409d4](https://github.com/moinulmoin/voicetypr/commit/22409d41cd5a0396b072817a9cb04ce01c97b5db))
* :sparkles: clean up release-universal.sh by commenting out test command and removing unnecessary blank lines for improved readability ([0d39040](https://github.com/moinulmoin/voicetypr/commit/0d390404b46c955110ea4870dc09e31413a8c211))
* :sparkles: dd autostart functionality with settings toggle, update dependencies, and improve UI components for better user experience ([da24d37](https://github.com/moinulmoin/voicetypr/commit/da24d373601325891da7c0026decaae17ec2d6cc))
* :sparkles: enhance audio recording features with real-time audio level visualization, implement update checks in AboutSection, and integrate tauri-plugin-updater for seamless application updates ([6494a96](https://github.com/moinulmoin/voicetypr/commit/6494a96a33c01c181c4939d3927112956a5b248e))
* :sparkles: enhance audio visualization in AudioWaveAnimation with improved animation logic and state management, and refactor RecordingPill for better feedback handling and cleanup on unmount ([971c9ee](https://github.com/moinulmoin/voicetypr/commit/971c9ee6bd0ae2357b6f28a55a22986624615ccd))
* :sparkles: enhance transcription management with cleanup settings and history retrieval, improve UI with pill widget toggle, and update tests for new settings ([943d398](https://github.com/moinulmoin/voicetypr/commit/943d398219063081bdd78fe2a019d4dc44053149))
* :sparkles: enhance transcription management with cleanup settings and history retrieval, improve UI with pill widget toggle, and update tests for new settings ([9ffa3f2](https://github.com/moinulmoin/voicetypr/commit/9ffa3f2a463a193a3952fb7e7069e7f0ce8566b9))
* :sparkles: implement download cancellation feature, enhance download progress management, and improve hotkey input handling ([3ea8ab8](https://github.com/moinulmoin/voicetypr/commit/3ea8ab83a94be0b59b5e5b1ac7be414dd841ce7d))
* :sparkles: implement model sorting and management enhancements, including balanced performance scoring, model deletion, and improved hotkey input handling ([bfcbb86](https://github.com/moinulmoin/voicetypr/commit/bfcbb8630f1174c70bee64f3ce2670f15f05412f))
* :sparkles: implement transcription deletion feature, enhance AboutSection with update checks and external links, and improve RecentRecordings component with history refresh ([be5b9e6](https://github.com/moinulmoin/voicetypr/commit/be5b9e67ee2cf64957184aacb0bf11e966d8cb91))
* :sparkles: improve UI components and enhance error handling in recording and model management ([6d26e9a](https://github.com/moinulmoin/voicetypr/commit/6d26e9a94e93893caec6a610cf2d1a73dfbebf9c))
* :sparkles: improvements ([a042c86](https://github.com/moinulmoin/voicetypr/commit/a042c863711642390cb1a0d6e153431d03b9e6e7))
* :sparkles: init ([3089d43](https://github.com/moinulmoin/voicetypr/commit/3089d43a5926f0eb4b11b33f1de1d8c029b7ed1c))
* :sparkles: integrate react-error-boundary for enhanced error handling and improve performance with useCallback and useMemo optimizations in App component ([cc92f93](https://github.com/moinulmoin/voicetypr/commit/cc92f931fe5be2d70a80340d16456c0ff626b614))
* :sparkles: make the pill working right ([010d96f](https://github.com/moinulmoin/voicetypr/commit/010d96f839c5d4ebab04d290661ef41184d43fe6))
* :sparkles: nhance RecordingPill with feedback messages for transcription events, improve ESC key handling for recording cancellation, and update GeneralSettings with tips for users ([89a9e07](https://github.com/moinulmoin/voicetypr/commit/89a9e07b8601bad91017782d928e2191f4db42a2))
* :sparkles: nhance VoiceTypr with new features, including global shortcut support, model management, and improved UI for recording and settings ([ca47873](https://github.com/moinulmoin/voicetypr/commit/ca47873a46856ec5e72c0fffee8286aa79f2a2c6))
* :sparkles: refactor GitHub Actions workflow for improved macOS target handling and streamline dependency installation ([c89eaff](https://github.com/moinulmoin/voicetypr/commit/c89eaff4bb4a0757551101495060fa73921ca31f))
* :sparkles: remove recording form the window ([5d51fae](https://github.com/moinulmoin/voicetypr/commit/5d51faef30a32ba8a676bc819538ed56ae0f1c3d))
* :sparkles: reorganize imports and enhance transcription handling in App and useRecording hook ([5b217a1](https://github.com/moinulmoin/voicetypr/commit/5b217a172eb4ac6007f0a396ae8769236c91ebca))
* :sparkles: update app title to VoiceTypr, add framer-motion and geist dependencies, and remove unused SVG files to streamline project assets ([04c41ee](https://github.com/moinulmoin/voicetypr/commit/04c41ee61c9b1479593350b25301a92f8bdfec6b))
* :sparkles: update dependencies in package.json, add styles to pill.html for full height layout, and remove unused components to streamline the codebase ([747d8bf](https://github.com/moinulmoin/voicetypr/commit/747d8bfb2f94784b77a3b854c41cb708c1acba84))
* :sparkles: update IOSSpinner component for improved animation timing and adjust RecordingPill layout for better alignment ([a4f269b](https://github.com/moinulmoin/voicetypr/commit/a4f269bdd8365c78c7c3b42093e9a049c44d21a1))
* :sparkles: update window size for better visibility, remove unused download cleanup logic, and streamline cancellation handling ([3ed8ac0](https://github.com/moinulmoin/voicetypr/commit/3ed8ac026db9b69fbbfb4d0dc711e96ff0ec06b6))
