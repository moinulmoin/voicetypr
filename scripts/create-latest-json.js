#!/usr/bin/env node

// Script to create combined latest.json for auto-updater
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const version = process.argv[2];
const outputDir = process.argv[3];

if (!version || !outputDir) {
  console.error('Usage: node create-latest-json.js <version> <output-dir>');
  process.exit(1);
}

// Read signatures from .sig files
function readSignature(archPath) {
  const sigPath = `${archPath}.sig`;
  if (fs.existsSync(sigPath)) {
    return fs.readFileSync(sigPath, 'utf8').trim();
  }
  return null;
}

// Find the app.tar.gz files
const x64Path = path.join(outputDir, `VoiceTypr_${version}_x64.app.tar.gz`);
const aarch64Path = path.join(outputDir, `VoiceTypr_${version}_aarch64.app.tar.gz`);

const x64Sig = readSignature(x64Path);
const aarch64Sig = readSignature(aarch64Path);

if (!x64Sig || !aarch64Sig) {
  console.error('Error: Could not find signature files');
  console.error(`Looking for: ${x64Path}.sig and ${aarch64Path}.sig`);
  process.exit(1);
}

// Create latest.json
const latestJson = {
  version: version,
  notes: `See the full changelog at https://github.com/moinulmoin/voicetypr/releases/tag/v${version}`,
  pub_date: new Date().toISOString(),
  platforms: {
    "darwin-x86_64": {
      signature: x64Sig,
      url: `https://github.com/moinulmoin/voicetypr/releases/download/v${version}/VoiceTypr_${version}_x64.app.tar.gz`
    },
    "darwin-aarch64": {
      signature: aarch64Sig,
      url: `https://github.com/moinulmoin/voicetypr/releases/download/v${version}/VoiceTypr_${version}_aarch64.app.tar.gz`
    }
  }
};

// Write latest.json
const outputPath = path.join(outputDir, 'latest.json');
fs.writeFileSync(outputPath, JSON.stringify(latestJson, null, 2));

console.log(`✅ Created ${outputPath}`);
console.log(`📋 Version: ${version}`);
console.log(`🔑 x64 signature: ${x64Sig.substring(0, 20)}...`);
console.log(`🔑 aarch64 signature: ${aarch64Sig.substring(0, 20)}...`);