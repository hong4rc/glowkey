---
phase: 3
title: "File permissions on the log and settings"
status: pending
priority: P1
effort: "4-6h"
dependencies: []
---

# Phase 3: File permissions on the log and settings

Executes [`260905-1643` §2.3](../260905-1643-glowkey-production-hardening/phase-2-privacy-and-logging.md)
("Restrict permissions (unconditional)"). That section is the design of record.

## Overview

Explicit `0600` on the log and settings files on Unix; a restrictive DACL on
Windows. Today protection is incidental on one platform and absent on the other.

§2.3 is marked **unconditional** — unlike §2.4 (redaction), it waits on no
decision. §2.2 already stopped the log recording typed text by default, so this
is defence for the opt-in diagnostic case and for the settings file, not the
last line against a plaintext keylog. It is still the highest-value unblocked
code in the plan: this is a keyboard tool, and a world-readable diagnostic log
is the worst-shaped file it owns.

## Requirements

- Functional: on Unix the log and settings files are mode `0600` — owner
  read/write, nothing for group or other.
- Functional: on Windows both carry a DACL granting the owning user only, with
  inheritance from the parent directory not silently re-widening it.
- Functional: permissions are set **at creation**, not after the first write. A
  file that is briefly world-readable is a file that leaked.
- Non-functional: a permission failure must never break typing or lose settings.
  Logging is already best-effort (`open_appending` returns `None` and logging
  silently does nothing); settings must not become unsaveable because a DACL
  call failed.
- Non-functional: existing files created by earlier versions are tightened on
  next open, not left at their old mode.

## Architecture

Two creation sites, and they have different failure budgets.

**The log — `app/src/log.rs:95`, `open_appending`:**

```rust
let file = OpenOptions::new()
    .create(true)
    .append(true)
    .open(path)
    .ok()?;
```

Unix: add `.mode(0o600)` via `std::os::unix::fs::OpenOptionsExt`. Note this
applies **only on creation** — an existing file keeps its mode, so an explicit
`set_permissions` is still needed for files from earlier versions.

Failure budget: generous. The function already returns `Option` and logging is
documented as "must never affect typing". If tightening fails, prefer *not
logging* over logging into a file that cannot be protected — that is the
privacy-preserving direction, and it is the one place in the codebase where
dropping the feature is the safe default.

**The settings file — `app/src/settings_store.rs:123`, `write_durably`:**

```rust
let mut file = fs::File::create(path)?;
```

This is the write-to-temp side of a write-then-rename. Two consequences worth
handling deliberately:

1. The **temp file** must be created restricted, not the final path only.
   Tightening after the rename leaves a window where the real content sits at
   default permissions.
2. `File::create` truncates and uses default permissions on a *new* file; the
   rename then replaces the target. On Windows the replaced file's DACL does not
   automatically carry over, so the DACL belongs on the temp file before the
   rename.

Failure budget: tight. Settings must still save. If the permission call fails,
save anyway and log it — losing the user's settings to protect their mode bits
is the wrong trade, and phase 1 of `260905-1643` already fixed a bug where the
settings path destroyed its own backup.

**Windows DACL — decided 2026-09-06: explicit per-file DACL.** The Rust standard
library has no DACL API; `std::fs::set_permissions` on Windows only toggles the
read-only attribute, which is **not** access control and must not be mistaken
for it.

Use `SetNamedSecurityInfoW` with an explicit DACL built from the current user's
SID, applied per file. Chosen over a directory DACL plus inheritance because
inheritance is exactly the mechanism that silently re-widens later, and because
direct API calls are already how `platform/windows` is written throughout.

Verified before deciding: `app/Cargo.toml` already enables the `Win32_Security`
`windows-sys` feature (for reading our own integrity level in the UIPI check),
and there is **no existing DACL or `SECURITY_ATTRIBUTES` call anywhere** in
`app/` or `crates/` — so this is new ground, and the incidental protection the
files have today comes only from their location. `SetNamedSecurityInfoW` may
additionally need `Win32_Security_Authorization`; add that feature only if the
build asks for it.

**Platform verification asymmetry, stated plainly:** the Windows half is
verifiable here. The Unix half is not — there is no Mac in this environment, and
`0600` on macOS is asserted by a `#[cfg(unix)]` test that cannot run on this
machine. The Unix code ships written-and-unverified and its sign-off belongs to
`260905-1643` phase 5. Do not report this phase as verified on both platforms.

## Related Code Files

- Modify: `app/src/log.rs` — `open_appending` (~:91-102), plus tightening of an
  existing file
- Modify: `app/src/settings_store.rs` — `write_durably` (~:122-126), on the temp
  path before the rename
- Modify: `app/src/platform/windows/` — a small DACL helper, placed beside the
  existing direct-API modules rather than in shared code, since it is
  Windows-only
- Modify: `app/Cargo.toml` — only to add `Win32_Security_Authorization` if the
  build asks for it. `Win32_Security` is already enabled (verified), so no other
  feature change is expected
- Read only: `app/src/platform/windows/paths.rs` — where the two paths come from

## Implementation Steps

1. Confirm both real paths on both platforms from `paths.rs` and the macOS
   equivalent, so the tests assert against the shipping locations.
2. Unix: add `.mode(0o600)` to `open_appending`, and `set_permissions` for an
   already-existing log. Same for the settings temp file.
3. Windows: implement the `SetNamedSecurityInfoW` helper per the decision above;
   apply it to the log on create and to the settings **temp** file before the
   rename.
4. Tests:
   - `#[cfg(unix)]` — create both files, assert `mode() & 0o777 == 0o600`.
   - `#[cfg(unix)]` — pre-create a file `0644`, open, assert it is tightened.
   - `#[cfg(windows)]` — create both, read the DACL back, assert no ACE grants
     access beyond the owning user. Reading it back is the point; asserting the
     call returned `Ok` proves nothing about the resulting permissions.
   - Both — a settings save whose permission call fails still writes the
     settings and logs the failure.
5. Verify on Windows that a settings save survives the rename with the DACL
   intact — this is the step most likely to be silently wrong.
6. `cargo test -p glowkey` and `cargo clippy`.

## Success Criteria

- [ ] On Windows, both files' DACLs grant the owning user only, asserted by
      reading the DACL back after a real save and a real log write
- [ ] The settings DACL survives the write-then-rename
- [ ] `#[cfg(unix)]` tests assert `0600` on create and tightening on open
- [ ] A failed permission call loses neither settings nor typing; the log records it
- [ ] `cargo clippy` clean
- [ ] The phase records that the Unix half is unverified on hardware, and
      `260905-1643` phase 5 lists it

## Risk Assessment

| Risk | Signal it broke | Response |
|---|---|---|
| `std::fs::set_permissions` used on Windows and mistaken for access control | Test passes; file is still readable by other users | Never assert on the call's return. Every Windows test reads the DACL back. This is the phase's single most likely silent failure |
| The rename drops the DACL, leaving the real settings file unprotected | Read-back after a save shows default permissions | Apply the DACL to the temp file *and* assert after the rename, not before |
| A permission failure makes settings unsaveable | Settings silently stop persisting; user loses configuration | Save-then-log-failure ordering, explicitly tested. Never gate the write on the permission call |
| A later change moves to directory inheritance and re-widens permissions | A file created after some unrelated directory change is readable | The decision is explicit per-file DACLs; inheritance was rejected for this reason. Any move to inheritance must bring a test that creates a file through the *real* code path and reads *its* DACL, not the directory's |
| Unix half reported as done when it was never executed | A macOS release ships an unverified `0600` claim | Phase records the asymmetry; sign-off is `260905-1643` phase 5's, and this plan's success criteria say so |
