#!/usr/bin/env node
// Ensures ffmpeg/ffprobe sidecar binaries exist without relying on system installs.
// Downloads pinned archives per platform, extracts, and places binaries under sidecar/ffmpeg/dist.
// macOS (arm64): sidecar/ffmpeg/dist/ffmpeg, ffprobe (+ arch symlinks)
// Windows (x64): sidecar/ffmpeg/dist/ffmpeg.exe, ffprobe.exe (+ arch copies)
const fs = require('fs');
const path = require('path');
const os = require('os');
const cp = require('child_process');
const crypto = require('crypto');

function ensureDir(p) {
  if (!fs.existsSync(p)) fs.mkdirSync(p, { recursive: true });
}

function run(cmd, args, cwd) {
  cp.execFileSync(cmd, args, { stdio: 'inherit', cwd });
}

function download(url, outFile) {
  console.log(`[ensure-ffmpeg-sidecar] Downloading ${url}`);
  // Use curl if available
  try {
    // -sS: silent but show errors; -L: follow redirects; -f: fail on HTTP errors
    run('curl', ['-sS', '-L', '-f', '-o', outFile, url]);
    return;
  } catch (e) {
    // Fallback to powershell on Windows
    if (process.platform === 'win32') {
      cp.execSync(
        `powershell -Command "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -UseBasicParsing -Uri '${url}' -OutFile '${outFile.replace(/'/g, "''")}'"`,
        { stdio: 'inherit' }
      );
      return;
    }
    throw e;
  }
}

function unzip(zipFile, outDir) {
  ensureDir(outDir);
  if (process.platform === 'win32') {
    // Use PowerShell Expand-Archive
    cp.execSync(
      `powershell -Command "Expand-Archive -Path '${zipFile.replace(/'/g, "''")}' -DestinationPath '${outDir.replace(/'/g, "''")}' -Force"`,
      { stdio: 'inherit' }
    );
  } else {
    // macOS has unzip by default
    run('unzip', ['-o', zipFile, '-d', outDir]);
  }
}

function chmodx(p) {
  try { if (process.platform !== 'win32') fs.chmodSync(p, 0o755); } catch (_) {}
}

function ensureSymlink(target, link) {
  try {
    if (fs.existsSync(link)) fs.unlinkSync(link);
    fs.symlinkSync(path.basename(target), link);
  } catch (err) {
    console.warn(`[ensure-ffmpeg-sidecar] Failed to create symlink ${link}: ${err}`);
  }
}

function ensureCopy(src, dst) {
  try { if (!fs.existsSync(dst)) fs.copyFileSync(src, dst); } catch (err) {
    console.warn(`[ensure-ffmpeg-sidecar] Failed to create copy ${dst}: ${err}`);
  }
}

function fail(msg) {
  console.error(`\n[ensure-ffmpeg-sidecar] ${msg}\n`);
  process.exit(1);
}

function sha256File(file) {
  const buf = fs.readFileSync(file);
  return crypto.createHash('sha256').update(buf).digest('hex');
}

function verifyChecksum(file, expected, label) {
  if (!expected) {
    console.log(`[ensure-ffmpeg-sidecar] Skipping checksum for ${label} (no expected hash provided).`);
    return;
  }
  const actual = sha256File(file);
  if (actual.toLowerCase() !== expected.toLowerCase()) {
    fail(`${label} checksum mismatch. Expected ${expected}, got ${actual}`);
  }
  console.log(`[ensure-ffmpeg-sidecar] Verified ${label} SHA256: ${actual}`);
}

(function main() {
  const distDir = path.resolve(__dirname, '..', 'sidecar', 'ffmpeg', 'dist');
  ensureDir(distDir);

  const isMac = process.platform === 'darwin';
  const isWin = process.platform === 'win32';

  if (isMac) {
    if (process.arch !== 'arm64') {
      console.warn('[ensure-ffmpeg-sidecar] Non-arm64 mac detected; this project targets Apple Silicon.');
    }
    const ffmpegDst = path.join(distDir, 'ffmpeg');
    const ffprobeDst = path.join(distDir, 'ffprobe');
    if (fs.existsSync(ffmpegDst) && fs.existsSync(ffprobeDst)) {
      // Ensure arch-specific symlinks even if binaries already present
      ensureSymlink(ffmpegDst, path.join(distDir, 'ffmpeg-aarch64-apple-darwin'));
      ensureSymlink(ffprobeDst, path.join(distDir, 'ffprobe-aarch64-apple-darwin'));
      console.log('[ensure-ffmpeg-sidecar] ffmpeg/ffprobe sidecars present. Symlinks ensured.');
      return;
    }

    // Pinned macOS Apple Silicon 8.0 from OSXExperts.NET (override via env if needed)
    const ffmpegZipUrl = process.env.FFMPEG_MAC_URL || 'https://www.osxexperts.net/ffmpeg80arm.zip';
    const ffprobeZipUrl = process.env.FFPROBE_MAC_URL || 'https://www.osxexperts.net/ffprobe80arm.zip';

    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'fftools-'));
    const ffmpegZip = path.join(tmp, 'ffmpeg.zip');
    const ffprobeZip = path.join(tmp, 'ffprobe.zip');
    download(ffmpegZipUrl, ffmpegZip);
    download(ffprobeZipUrl, ffprobeZip);
    const outFfmpeg = path.join(tmp, 'ffmpeg');
    const outFfprobe = path.join(tmp, 'ffprobe');
    unzip(ffmpegZip, tmp);
    unzip(ffprobeZip, tmp);

    // OSXExperts zips contain a single binary named ffmpeg/ffprobe at tmp
    if (!fs.existsSync(outFfmpeg) || !fs.existsSync(outFfprobe)) {
      fail('Downloaded archives did not contain expected ffmpeg/ffprobe binaries.');
    }
    // Verify checksums against extracted binaries (provider publishes hashes for binaries)
    const macFfmpegBinSha = process.env.FFMPEG_MAC_BIN_SHA256 || '77d2c853f431318d55ec02676d9b2f185ebfdddb9f7677a251fbe453affe025a';
    const macFfprobeBinSha = process.env.FFPROBE_MAC_BIN_SHA256 || 'babf170e86bd6b0b2fefee5fa56f57721b0acb98ad2794b095d8030b02857dfe';
    verifyChecksum(outFfmpeg, macFfmpegBinSha, 'macOS ffmpeg (binary)');
    verifyChecksum(outFfprobe, macFfprobeBinSha, 'macOS ffprobe (binary)');
    fs.copyFileSync(outFfmpeg, ffmpegDst); chmodx(ffmpegDst);
    fs.copyFileSync(outFfprobe, ffprobeDst); chmodx(ffprobeDst);

    ensureSymlink(ffmpegDst, path.join(distDir, 'ffmpeg-aarch64-apple-darwin'));
    ensureSymlink(ffprobeDst, path.join(distDir, 'ffprobe-aarch64-apple-darwin'));
    console.log('[ensure-ffmpeg-sidecar] Installed macOS sidecar binaries by download.');
    return;
  }

  if (isWin) {
    const ffmpegDst = path.join(distDir, 'ffmpeg.exe');
    const ffprobeDst = path.join(distDir, 'ffprobe.exe');
    if (fs.existsSync(ffmpegDst) && fs.existsSync(ffprobeDst)) {
      // Ensure arch-named copies for bundler compatibility
      ensureCopy(ffmpegDst, path.join(distDir, 'ffmpeg-x86_64-pc-windows-msvc.exe'));
      ensureCopy(ffprobeDst, path.join(distDir, 'ffprobe-x86_64-pc-windows-msvc.exe'));
      console.log('[ensure-ffmpeg-sidecar] ffmpeg/ffprobe sidecars present. Copies ensured.');
      return;
    }

    // Use gyan.dev essentials latest (or override). This is version-agnostic latest link.
    const zipUrl = process.env.FFMPEG_WIN_URL || 'https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip';
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'fftools-'));
    const zipFile = path.join(tmp, 'ffmpeg.zip');
    download(zipUrl, zipFile);
    // Optional checksum verification (provide env or attempt to fetch .sha256 sidecar)
    let winSha = process.env.FFMPEG_WIN_ZIP_SHA256;
    if (!winSha) {
      try {
        const shaFile = path.join(tmp, 'ffmpeg.zip.sha256');
        const shaUrl = zipUrl + '.sha256';
        download(shaUrl, shaFile);
        const txt = fs.readFileSync(shaFile, 'utf8');
        // Typical format: <sha256> *ffmpeg-release-essentials.zip
        winSha = (txt.split(/\s+/)[0] || '').trim();
      } catch (_) {}
    }
    if (winSha) verifyChecksum(zipFile, winSha, 'Windows ffmpeg.zip');
    const outDir = path.join(tmp, 'out');
    unzip(zipFile, outDir);

    // Find bin folder inside extracted structure
    const entries = fs.readdirSync(outDir);
    const root = entries.find((e) => e.toLowerCase().startsWith('ffmpeg-') && fs.statSync(path.join(outDir, e)).isDirectory());
    if (!root) fail('Unexpected archive structure for Windows ffmpeg build.');
    const binDir = path.join(outDir, root, 'bin');
    const srcFfmpeg = path.join(binDir, 'ffmpeg.exe');
    const srcFfprobe = path.join(binDir, 'ffprobe.exe');
    if (!fs.existsSync(srcFfmpeg) || !fs.existsSync(srcFfprobe)) {
      fail('Downloaded Windows archive missing ffmpeg.exe/ffprobe.exe');
    }
    fs.copyFileSync(srcFfmpeg, ffmpegDst);
    fs.copyFileSync(srcFfprobe, ffprobeDst);
    ensureCopy(ffmpegDst, path.join(distDir, 'ffmpeg-x86_64-pc-windows-msvc.exe'));
    ensureCopy(ffprobeDst, path.join(distDir, 'ffprobe-x86_64-pc-windows-msvc.exe'));
    console.log('[ensure-ffmpeg-sidecar] Installed Windows sidecar binaries by download.');
    return;
  }

  console.warn('[ensure-ffmpeg-sidecar] Unsupported platform for auto-install; please place binaries under sidecar/ffmpeg/dist/.');
})();
