#!/usr/bin/env node
/*
 Ensures ffmpeg/ffprobe sidecar binaries exist for dev.
 - macOS (arm64): sidecar/ffmpeg/dist/ffmpeg, ffprobe
 - Windows (x64): sidecar/ffmpeg/dist/ffmpeg-x86_64-pc-windows-msvc.exe, ffprobe-x86_64-pc-windows-msvc.exe
 If missing, tries to locate system ffmpeg/ffprobe and copies them into dist.
 Fails with a clear message if not found.
*/
const fs = require('fs');
const path = require('path');
const cp = require('child_process');

function ensureDir(p) {
  if (!fs.existsSync(p)) fs.mkdirSync(p, { recursive: true });
}

function which(cmd) {
  try {
    if (process.platform === 'win32') {
      const out = cp.execSync(`where ${cmd}`, { stdio: ['ignore', 'pipe', 'ignore'] }).toString().split(/\r?\n/)[0].trim();
      return out || null;
    }
    const out = cp.execSync(`which ${cmd}`, { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim();
    return out || null;
  } catch (_) { return null; }
}

function copyIfMissing(src, dst) {
  if (!src || !fs.existsSync(src)) return false;
  fs.copyFileSync(src, dst);
  if (process.platform !== 'win32') {
    try { fs.chmodSync(dst, 0o755); } catch (_) {}
  }
  return true;
}

function fail(msg) {
  console.error(`\n[ensure-ffmpeg-sidecar] ${msg}\n`);
  process.exit(1);
}

(function main() {
  const distDir = path.resolve(process.cwd(), 'sidecar/ffmpeg/dist');
  ensureDir(distDir);

  if (process.platform === 'darwin') {
    if (process.arch !== 'arm64') {
      console.warn('[ensure-ffmpeg-sidecar] Non-arm64 mac detected; this project targets Apple Silicon.');
    }
    const ffmpegDst = path.join(distDir, 'ffmpeg');
    const ffprobeDst = path.join(distDir, 'ffprobe');

    const missing = [];
    if (!fs.existsSync(ffmpegDst)) missing.push('ffmpeg');
    if (!fs.existsSync(ffprobeDst)) missing.push('ffprobe');

    if (missing.length === 0) {
      console.log('[ensure-ffmpeg-sidecar] ffmpeg/ffprobe sidecars present.');
      return;
    }

    // Try common Homebrew paths first
    const brewFfmpeg = '/opt/homebrew/bin/ffmpeg';
    const brewFfprobe = '/opt/homebrew/bin/ffprobe';
    const sysFfmpeg = which('ffmpeg');
    const sysFfprobe = which('ffprobe');

    let ok = true;
    if (!fs.existsSync(ffmpegDst)) {
      ok = copyIfMissing(brewFfmpeg, ffmpegDst) || (sysFfmpeg && copyIfMissing(sysFfmpeg, ffmpegDst));
    }
    if (!fs.existsSync(ffprobeDst)) {
      ok = (copyIfMissing(brewFfprobe, ffprobeDst) || (sysFfprobe && copyIfMissing(sysFfprobe, ffprobeDst))) && ok;
    }
    if (!ok) {
      fail('Missing ffmpeg/ffprobe. Install via Homebrew: `brew install ffmpeg`, or place Apple Silicon binaries at sidecar/ffmpeg/dist/.');
    }
    console.log('[ensure-ffmpeg-sidecar] Installed macOS sidecar binaries from system.');
    return;
  }

  if (process.platform === 'win32') {
    const ffmpegDst = path.join(distDir, 'ffmpeg.exe');
    const ffprobeDst = path.join(distDir, 'ffprobe.exe');
    const missing = [];
    if (!fs.existsSync(ffmpegDst)) missing.push('ffmpeg');
    if (!fs.existsSync(ffprobeDst)) missing.push('ffprobe');
    if (missing.length === 0) {
      console.log('[ensure-ffmpeg-sidecar] ffmpeg/ffprobe sidecars present.');
      return;
    }
    const sysFfmpeg = which('ffmpeg');
    const sysFfprobe = which('ffprobe');
    let ok = true;
    if (!fs.existsSync(ffmpegDst)) ok = sysFfmpeg && copyIfMissing(sysFfmpeg, ffmpegDst);
    if (!fs.existsSync(ffprobeDst)) ok = sysFfprobe && copyIfMissing(sysFfprobe, ffprobeDst) && ok;
    if (!ok) {
      fail('Missing ffmpeg/ffprobe. Install via Chocolatey: `choco install ffmpeg`, or place Windows x64 binaries at sidecar/ffmpeg/dist/.');
    }
    console.log('[ensure-ffmpeg-sidecar] Installed Windows sidecar binaries from system.');
    return;
  }

  console.warn('[ensure-ffmpeg-sidecar] Unsupported platform for auto-install; please place binaries under sidecar/ffmpeg/dist/.');
})();
