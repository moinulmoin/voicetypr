# Plan 061 — License preservation — non-destructive secure-store reads

**Status:** IN PROGRESS
**Priority:** P0
**Effort:** S
**Depends on:** 060 (release remediation wave; independent file ownership)

## Problem (incident, Windows 2.0.5 — reproduced on main `4f9e497d`)

At startup the stored license decrypt failed and `secure_get`
(`src-tauri/src/secure_store.rs:145-195`) **deleted the saved record** and
returned `Ok(None)`. Consequence chain:

1. `check_license_status_impl` (`commands/license.rs:441`) only enters the paid
   branch on `Some`; `None` falls into the trial branch → server trial check →
   expired trial → recording blocked.
2. The license record was destroyed on disk, so the damage is permanent for the
   user (no in-app recovery; re-entering the key is the only path).
3. The same deletion applies to corrupt **API-key** entries (cloud STT / AI
   providers), silently discarding recoverable credentials.

Root cause of the decrypt failure itself is **not established** (wrong AES key
derivation vs. on-disk corruption — GCM auth failure is identical for both).
Parent evidence: injected decrypt failure with a synthetic temp-backed store →
`secure_get` returned `Ok(None)` twice, persisted entry gone, one decrypt call.

## Fix (non-destructive read; no crypto or device-identity redesign)

`secure_store.rs` only. Generic secure read distinguishes three outcomes that
today all collapse to "absent":

| Situation | Old behavior | New behavior |
|---|---|---|
| Key absent | `Ok(None)` | `Ok(None)` (unchanged; trial flow stays normal) |
| Store file unreadable/corrupt (whole file) | `Ok(None)` (store build silently drops the load error) | `Err("Secure store file could not be read: …")` |
| Value present, decrypt fails (auth/length/UTF-8) | **delete + save + `Ok(None)`** | `Err("… could not be decrypted … the saved entry was preserved")`, **no mutation** |
| Value present, wrong JSON type | **delete + `Ok(None)`** | `Err("… unexpected format …")`, **no mutation** |
| `app.store()` access error | `Ok(None)` | `Err("Secure store is unavailable: …")` |

Mechanics: extract `read_secure_value` over a minimal `SecureStoreBackend` seam
(`raw_value` + `refresh`); `secure_get`/`secure_has` call it with the real
plugin store. `refresh` = `store.reload()` with `ErrorKind::NotFound` mapped to
"fresh install" — run only when the key is unknown to the cached view, so a
missing entry is reported only after the file itself is readable. Reads never
call `delete`/`save`; `secure_delete`, deactivation, and Reset keep their
explicit user-action semantics (and `secure_delete`'s `"Failed to access store"`
error prefix, which `reset.rs:101` matches on).

Consumer audit (all `secure_get`/`secure_has` callers, no changes needed):
`license/keychain.rs` (`?` propagates → `check_license_status`/`restore`/
`deactivate`/`activate` read-back), `commands/ai.rs` warm cache (Err arm logs,
skips), `cloud_stt/mod.rs` (`?` → transcription error), `commands/remote.rs`
(explicit Err arm fails closed), `transcription/executor.rs` (missing-key
error), `commands/keyring.rs` (honest error to settings UI), `audio.rs`/
`model.rs`/`tray.rs`/`model_selection.rs` (`unwrap_or(false)` fail-safe).
Frontend: `LicenseContext.checkStatus` on command error toasts the message and
keeps `status = null` → app-not-ready; **no falsely expired / no-license state**,
so no frontend change. Error messages carry key names and plugin error text
only — never ciphertext, plaintext keys, or device fingerprints.

## Tests (`secure_store.rs`, real AES-GCM boundary via the seam)

1. Valid-length authentication failure (ciphertext byte flipped after the
   nonce) → distinct Err, saved record unchanged, evidence survives repeat read.
2. Invalid value type (non-string JSON) → distinct Err, record unchanged.
3. Missing entry → `Ok(None)` normal path.
4. Unreadable store file (`refresh` failure) → Err distinct from absent.

## Out of scope / explicit non-goals

- No `device.rs`, PBKDF2, or device-hash changes; no new key-migration scheme.
- No entitlement bypass or auto-activation; unreadable ≠ licensed.
- Server-revocation behavior (`should_delete_invalid_license`) untouched.
- Already-deleted licenses are **not** recoverable by this fix (data is gone).
- Residual unknown: why decryption failed (wrong key vs. corruption). Safe user
  guidance: license record preserved → update/reinstall the app, retry; if the
  error persists, re-enter the license key (server re-activation) or contact
  support; do not delete `secure.dat` manually (other keys live there).
