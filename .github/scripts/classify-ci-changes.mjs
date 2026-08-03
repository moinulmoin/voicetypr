import fs from 'node:fs';

const FAST_PATH_PREFIXES = ['.github/', 'docs/', 'plans/'];
const ROOT_DOCUMENTATION = /^[^/]+\.md$/i;

export function isWorkflowOrDocumentationPath(filePath) {
  const normalized = filePath.replaceAll('\\', '/').replace(/^\.\//, '');
  return (
    FAST_PATH_PREFIXES.some((prefix) => normalized.startsWith(prefix)) ||
    ROOT_DOCUMENTATION.test(normalized)
  );
}

export function classifyChangedFiles(filePaths) {
  const changedFiles = filePaths.filter((filePath) => filePath.length > 0);

  if (changedFiles.length === 0) {
    return {
      applicationRequired: true,
      reason: 'No changed files were detected; running application checks conservatively.',
    };
  }

  const applicationFiles = changedFiles.filter(
    (filePath) => !isWorkflowOrDocumentationPath(filePath),
  );

  if (applicationFiles.length > 0) {
    return {
      applicationRequired: true,
      reason: `Application/build inputs changed: ${applicationFiles.join(', ')}`,
    };
  }

  return {
    applicationRequired: false,
    reason: 'Only workflow or documentation files changed.',
  };
}

function appendOutput(name, value) {
  const outputPath = process.env.GITHUB_OUTPUT;
  if (!outputPath) return;
  fs.appendFileSync(outputPath, `${name}=${value}\n`);
}

function main() {
  const nullDelimited = process.argv.includes('--null');
  const input = fs.readFileSync(0, 'utf8');
  const filePaths = input.split(nullDelimited ? '\0' : /\r?\n/);
  const result = classifyChangedFiles(filePaths);

  appendOutput('application_required', String(result.applicationRequired));
  console.log(result.reason);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main();
}
