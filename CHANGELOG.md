# Changelog

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
