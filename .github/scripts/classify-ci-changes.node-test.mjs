import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import {
  classifyChangedFiles,
  isWorkflowOrDocumentationPath,
} from './classify-ci-changes.mjs';

describe('CI change classification', () => {
  it('recognizes workflow, plan, docs, and root documentation paths', () => {
    for (const filePath of [
      '.github/workflows/release.yml',
      '.github/scripts/release-tool.mjs',
      'plans/042-release-workflow-speed.md',
      'docs/release.md',
      'README.md',
      'CHANGELOG.md',
    ]) {
      assert.equal(isWorkflowOrDocumentationPath(filePath), true, filePath);
    }
  });

  it('requires application checks for product and build inputs', () => {
    for (const filePath of [
      'src/App.tsx',
      'src-tauri/src/lib.rs',
      'sidecar/parakeet-swift/Package.swift',
      'scripts/ensure-ffmpeg-sidecar.cjs',
      'package.json',
      'pnpm-lock.yaml',
    ]) {
      assert.equal(
        classifyChangedFiles(['.github/workflows/ci.yml', filePath]).applicationRequired,
        true,
        filePath,
      );
    }
  });

  it('requires application checks for both sides of a source-to-docs rename', () => {
    assert.equal(
      classifyChangedFiles(['src/removed.ts', 'docs/removed.ts']).applicationRequired,
      true,
    );
  });

  it('uses the fast path only when every changed path is workflow or documentation', () => {
    const result = classifyChangedFiles([
      '.github/workflows/ci.yml',
      '.github/scripts/classify-ci-changes.mjs',
      'plans/042-release-workflow-speed.md',
    ]);

    assert.equal(result.applicationRequired, false);
  });

  it('runs application checks conservatively when the diff is empty', () => {
    assert.equal(classifyChangedFiles([]).applicationRequired, true);
  });
});
