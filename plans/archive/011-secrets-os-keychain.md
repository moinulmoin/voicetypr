# Plan 011: Migrate secrets from the device-derived AES store to OS credential storage

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 41351bd..HEAD -- src-tauri/src/secure_store.rs src-tauri/src/commands/keyring.rs src-tauri/src/license/keychain.rs src-tauri/src/commands/remote.rs src-tauri/Cargo.toml`
> On drift, compare "Current state" excerpts to live code first; mismatch = STOP.

## Status

- **Priority**: P2
- **Effort**: M-L
- **Risk**: MED-HIGH (credential migration; a bug strands API keys, licenses,
  and sharing passwords — the migration must be read-old/write-new/verify/
  delete-old, per key, never destructive on failure)
- **Depends on**: none (Step 0 is an investigation gate)
- **Category**: security
- **Planned at**: commit `41351bd`, 2026-06-10

## Why this matters

All app secrets — AI provider API keys (via the `keyring_*` commands),
the license key, the LAN sharing password, and saved remote-connection
passwords — are stored in `secure.dat`, AES-256-GCM encrypted with a key
derived (PBKDF2, static salt) from the **device hash**. Any local process
running as the user can read the app data directory and reproduce that
derivation; this is materially weaker than macOS Keychain / Windows
Credential Manager, where the OS gates access per app identity. The module
names (`keyring.rs`, `license/keychain.rs`) promise the stronger model. This
plan swaps the backend to the OS store with a safe per-key migration.

**Caveat the executor must respect**: `secure_store.rs:58-71` contains a
dead-code helper `check_migration_needed` documented as "migration from
keychain" — evidence the project may have *deliberately* moved OFF the OS
keychain before (keychain prompts on unsigned dev builds are a common
reason). Step 0 settles this before any code is written.

## Current state

- `src-tauri/src/secure_store.rs` — the only secret backend:
  - Key derivation `:18-56`: PBKDF2-HMAC-SHA256 over
    `device::get_device_hash()` with static salt
    `b"voicetypr-secure-store-v1"`, 100k iterations, cached in a `OnceCell`.
  - API: `secure_set` (`:129-142`), `secure_get` (`:145-195`, deletes
    corrupted entries and returns `Ok(None)`), `secure_delete` (`:198-209`),
    `secure_has` (`:212-233`). Storage: tauri-plugin-store file `secure.dat`.
- Consumers (all go through the four functions above):
  - `src-tauri/src/commands/keyring.rs` — Tauri commands `keyring_set/get/
    delete/has` with key validation (`validate_key`, `:5-32`); the frontend
    stores AI provider keys through these (key names chosen by frontend).
  - `src-tauri/src/license/keychain.rs` — `save_license`/`get_license`/
    `delete_license` under the fixed key `"license"`.
  - `src-tauri/src/commands/remote.rs:27-68` — `SHARING_PASSWORD_KEY =
    "remote_sharing_password"` and per-server
    `remote_connection_password_{server_id}` keys via small wrappers.
- Initialization: `secure_store::initialize_encryption_key()` is called from
  `lib.rs` startup (search `ENCRYPTION_INIT`).
- `src-tauri/Cargo.toml` has NO OS-credential crate today; relevant deps:
  `aes-gcm`, `pbkdf2`, `dirs`, `tauri-plugin-store`.
- App identifier (for the keychain service name): read it from
  `src-tauri/tauri.conf.json` (`identifier` field) in Step 1 — do not guess.

## Commands you will need

| Purpose            | Command                                          | Expected on success |
|--------------------|--------------------------------------------------|---------------------|
| Compile            | `cd src-tauri && cargo check`                    | exit 0              |
| Secrets tests      | `cd src-tauri && cargo test secrets`             | all pass            |
| Full backend tests | `cd src-tauri && cargo test`                     | all pass            |
| Rust format        | `cd src-tauri && cargo fmt --check`              | exit 0              |
| Clippy             | `cd src-tauri && cargo clippy -- -D warnings`    | exit 0              |

## Scope

**In scope**:
- New module `src-tauri/src/secrets.rs` (backend trait + OS backend +
  migration).
- `src-tauri/src/secure_store.rs` (demoted to legacy read-only source for
  migration).
- `src-tauri/src/commands/keyring.rs`, `src-tauri/src/license/keychain.rs`,
  `src-tauri/src/commands/remote.rs` (switch to the new module).
- `src-tauri/Cargo.toml` (add the `keyring` crate).
- `src-tauri/src/lib.rs` (initialization hook only).

**Out of scope**:
- Frontend — command names and shapes (`keyring_set` etc.) must not change.
- The device-hash derivation in `license/device` (used for license API auth
  independently of storage).
- Deleting `secure.dat` wholesale (only migrated keys are removed
  individually).
- Windows/macOS UI for credential prompts.

## Git workflow

- Branch: `advisor/011-os-keychain-secrets`
- Commit message: `feat: store secrets in os credential manager with migration`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 0: INVESTIGATION GATE — was the move off OS keychain deliberate?

Run (read-only):

```
git log --oneline --follow -- src-tauri/src/secure_store.rs
git log --oneline --follow -- src-tauri/src/license/keychain.rs
git log -S "keychain" --oneline -- src-tauri/src/ | head -30
```

Read any commit whose message references keychain/keyring/prompt/migration
(`git show <sha> --stat`). **If you find evidence the project intentionally
abandoned the OS keychain for UX reasons (prompt fatigue, unsigned-dev-build
issues, corporate-managed devices), STOP and report the evidence instead of
implementing.** If history shows only the secure-store introduction with no
keychain predecessor rationale, proceed.

**Verify**: report the relevant commits + verdict before Step 1.

### Step 1: Add the `keyring` crate and the backend module

- `Cargo.toml`: add `keyring = "3"` (uses Security.framework on macOS,
  Windows Credential Manager on Windows; check the crate's current docs for
  required features — e.g. `features = ["apple-native", "windows-native"]`
  on v3 — and pin what compiles).
- Create `src-tauri/src/secrets.rs`:

```rust
pub trait SecretBackend: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
    fn delete(&self, key: &str) -> Result<(), String>;
}

pub struct OsKeychain { service: String }   // service = app identifier from tauri.conf.json
```

`OsKeychain` maps to `keyring::Entry::new(&self.service, key)`;
`Entry::get_password()` `NoEntry` error → `Ok(None)`; other errors →
`Err(string)`. Add an in-memory `MockBackend` under `#[cfg(test)]`.

**Verify**: `cargo check` exit 0 on macOS. (Windows compile is covered by CI
`cargo test --no-run`; note it in the report.)

### Step 2: Lazy per-key migration in the public API

In `secrets.rs`, the public functions the rest of the codebase will call:

```rust
pub fn secret_get(app: &AppHandle, key: &str) -> Result<Option<String>, String>
pub fn secret_set(app: &AppHandle, key: &str, value: &str) -> Result<(), String>
pub fn secret_delete(app: &AppHandle, key: &str) -> Result<(), String>
pub fn secret_has(app: &AppHandle, key: &str) -> Result<bool, String>
```

`secret_get` migration logic (per key, lazy, non-destructive on failure):

1. Try OS backend → `Some` → return it.
2. Else try legacy `secure_store::secure_get(app, key)` → `None` → return None.
3. `Some(value)`: write to OS backend; **read back and compare**; only on
   verified match, `secure_store::secure_delete(app, key)`; return the value.
   If the OS write or read-back fails: log a warning, return the value from
   legacy WITHOUT deleting (next call retries).

`secret_set` writes to the OS backend only, and best-effort deletes the
legacy entry. `secret_delete` deletes from both. `secret_has` = OS has ||
legacy has.

**Verify**: `cargo check` exit 0.

### Step 3: Switch the three consumers

Mechanical swap — `crate::secure_store::secure_*` → `crate::secrets::secret_*`
in:
- `commands/keyring.rs` (4 commands; keep `validate_key` untouched),
- `license/keychain.rs` (3 fns),
- `commands/remote.rs:33-68` (the three small wrappers; the rest of the file
  calls only those wrappers — verify with
  `grep -n "secure_store" src-tauri/src/commands/remote.rs`).

Then `grep -rn "secure_store::" src-tauri/src/` — remaining hits must be:
`secrets.rs` (the migration reads) and `lib.rs` initialization. Keep
`initialize_encryption_key()` at startup (legacy decryption still needs it
during the migration window).

**Verify**: `cargo check` exit 0; `cargo test` passes.

### Step 4: Tests

New `#[cfg(test)] mod` in `secrets.rs` using `MockBackend` for the OS side
and, for the legacy side, either the real `secure_store` against a test
store (the existing `secure_store.rs` tests at `:236-280` show
`initialize_encryption_key()` works in tests) or a second mock behind a small
seam — choose whichever the existing test harness supports without touching
production behavior; state the choice.

Cases:
1. `get_prefers_os_backend`.
2. `get_migrates_legacy_value` — legacy has it, OS empty → returns value, OS
   now has it, legacy entry gone.
3. `failed_os_write_keeps_legacy` — OS backend errors → value still returned,
   legacy entry retained.
4. `set_writes_os_and_clears_legacy`.
5. `delete_removes_both`.

**Verify**: `cd src-tauri && cargo test secrets` → all pass.

### Step 5: Full suite, format, clippy + manual smoke

**Verify**: `cargo test`, `cargo fmt --check`, `cargo clippy -- -D warnings`
all exit 0.

Manual smoke (macOS, REQUIRED): run the dev app with an existing
`secure.dat` containing at least one AI key → key still works; Keychain
Access shows the new item under the app identifier service; `secure.dat` no
longer contains that key after first use. Note: on an unsigned dev build,
macOS may prompt for keychain access on rebuild — REPORT whether a prompt
appeared and how often; if prompts appear on every rebuild, flag it
prominently (it's the suspected reason keychain was abandoned; the operator
decides whether to ship).

## Test plan

Five unit tests (Step 4) + manual migration smoke (Step 5). Existing remote/
license/keyring test suites (`cargo test remote`, `cargo test license`) must
stay green — they pin the command contracts.

## Done criteria

ALL must hold:

- [ ] Step 0 verdict recorded in the report (and in the index row)
- [ ] `keyring` crate integrated; `secrets.rs` exists with trait + OS backend
      + migration exactly as specified
- [ ] `grep -rn "secure_store::" src-tauri/src/` → only `secrets.rs` +
      `lib.rs` init
- [ ] 5 new tests pass; full `cargo test` passes; fmt + clippy clean
- [ ] macOS manual migration smoke performed; prompt behavior reported
- [ ] Only in-scope files modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- Step 0 finds deliberate-abandonment evidence.
- The `keyring` crate cannot target both macOS and Windows with the project's
  MSRV/toolchain (check `cargo check` errors) — do not substitute a different
  storage scheme on your own.
- The macOS smoke shows a keychain prompt on EVERY app launch (not just
  rebuilds) — that's a shipping blocker; report.
- Migrating reveals values >4 KB (Windows Credential Manager limit is ~2.5 KB
  per credential blob; `keyring_set` allows up to 10 MB per
  `keyring.rs:40-42`) — if any consumer actually stores large values, STOP:
  the size contract needs an operator decision.

## Maintenance notes

- After one or two releases, a cleanup pass can delete `secure_store.rs`'s
  encryption machinery and drop `aes-gcm`/`pbkdf2` if nothing else uses them
  (`grep` first) — deferred until telemetry/support confirms migrations
  completed.
- Reviewer should scrutinize: the read-back-verify-then-delete ordering in
  `secret_get`; that is the only line between "migration" and "data loss".
- The Windows credential size limit (see STOP) is the lurking constraint if
  anyone stores large blobs through `keyring_set`.
