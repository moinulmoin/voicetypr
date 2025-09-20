# Repository Guidelines

## Project Structure & Module Organization
VoiceTypr pairs a Vite-powered React frontend with a Rust-backed Tauri shell. Frontend code lives in `src/`, with UI components under `src/components`, shared context in `src/contexts`, hooks in `src/hooks`, and cross-cutting helpers in `src/lib` and `src/utils`. Assets such as icons and stylesheets sit in `src/assets` and `src/globals.css`. Integration harnesses are collected in `src/test`. Native logic resides in `src-tauri/src`, split into `audio`, `commands`, `whisper`, and `state` modules plus `tests` for Rust-side checks. Static files ship from `public/`, while automation and quality scripts live in `scripts/`.

## Build, Test, and Development Commands
Use `pnpm install` for setup. `pnpm dev` runs the web UI in Vite; pair it with `pnpm tauri dev` when you need the desktop shell. Produce a production bundle with `pnpm build`. Guard code quality via `pnpm lint`, `pnpm format`, and `pnpm typecheck`. Test suites run with `pnpm test` (Vitest), `pnpm test:watch` for TDD, and `pnpm test:backend` (Cargo) for Rust coverage. `pnpm quality-gate` executes the full preflight of linting, typing, and tests.

## Coding Style & Naming Conventions
TypeScript and React components use 2-space indentation, PascalCase filenames for components (e.g., `SettingsPanel.tsx`), and camelCase utilities. Favor function components with hooks and keep shared state inside context providers. Tailwind utility classes belong in JSX; fall back to `globals.css` for cross-cutting styles. Run Prettier (`pnpm format`) before committing, and let ESLint flag unsafe patterns such as missing dependency arrays. Rust modules follow `snake_case` filenames and 4-space indentation enforced by `cargo fmt`.

## Testing Guidelines
Vitest with React Testing Library drives frontend tests; co-locate specs as `*.test.tsx` inside the relevant feature folder or in `src/test` when covering multi-module flows. Ensure new UI flows include accessibility assertions. Use `pnpm test:coverage` to confirm meaningful branches/happy-paths are exercised (aim for ≥80% on touched files). Backend changes require `pnpm test:backend` and adding Rust integration tests under `src-tauri/src/tests`. Document any manual steps in test descriptions when hardware interactions are involved.

## Commit & Pull Request Guidelines
Follow Conventional Commits (`fix:`, `feat:`, `refactor:`) as seen in the log, keeping scopes tight and messages under 72 characters. Reference issues with `Fixes #NN` where applicable. Before opening a PR, run `pnpm quality-gate` and attach screenshots or screen recordings for UI-visible work. PRs should describe intent, note any migrations or capability changes in `src-tauri/capabilities`, and call out follow-up tasks so reviewers can focus on correctness and regressions.
