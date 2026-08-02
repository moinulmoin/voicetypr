# 042 — Release workflow speed

**Status: IN PROGRESS — claimed Main 2026-08-02**

## Problem

Workflow-only pull requests currently run the complete macOS and Windows native build matrix before the release workflow rebuilds and signs the same platforms. Release preparation logic is duplicated inline, and expensive sidecar inputs are not cached independently from the main Tauri build.

## Locked behavior

1. Every production release still builds, signs, and verifies macOS ARM64, macOS Intel, and Windows artifacts.
2. Pull requests that can affect application behavior still run the existing frontend and native CI matrix.
3. Workflow-only pull requests run dedicated workflow checks and release-logic tests instead of native application builds.
4. Caches may reuse dependency/build outputs but never final signed installers, signing material, or mutable downloaded binaries without integrity checks.
5. Exact tag matching and atomic stable publication remain enforced.

## Acceptance

- A deterministic change classifier distinguishes workflow-only changes from product/build-input changes.
- Required CI checks still report success for workflow-only pull requests without starting native build jobs.
- Release version calculation, beta validation, exact tag matching, and version-file mutation are implemented once and covered by Node tests.
- Windows Vulkan and Apple Silicon Parakeet build caches are keyed by all relevant source/dependency/toolchain inputs.
- FFmpeg download caching preserves the existing checksum/size validation path.
- Workflow syntax and focused tests pass, followed by an independent reviewer pass.
