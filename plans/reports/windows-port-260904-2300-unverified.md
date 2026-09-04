# What the Windows port has NOT verified, and why

Date: 2026-09-04
Branch: `feat/windows-input-core`
Covers: Phase 0 (engine tests) and Phase 4 (Windows input core) of
`plans/260904-2127-glowkey-cross-platform-port`, tracked by
[issue #1](https://github.com/hong4rc/glowkey/issues/1).

This is definition-of-done item 6 on that issue. It exists because the honest
answer to "does the Windows port work" is currently **unknown**, and an
unqualified green build is the most misleading thing this work could produce.

## What IS verified, on this machine

| Claim | Evidence |
|---|---|
| The Vietnamese engine runs correctly on Windows | `cargo test -p glowkey-engine` — 164 passed, 0 failed |
| The Windows backend compiles and links | `cargo build -p glowkey` produces a binary |
| Its own units are correct | `cargo test -p glowkey` — 19 passed, 0 failed |
| No lint or type errors | `cargo clippy --workspace --all-targets -- -D warnings` silent |
| The tag guard rejects every value but its own | `inject::tests`, a pure function over `dwExtraInfo` |
| Key mapping covers every key the ladder branches on | `adapt::tests`, one case per mapped key plus the range boundaries |
| Executable-name resolution matches the shipped table's spelling | `foreground::tests` |
| Integrity-level comparison is `>` and not `>=` | `elevation::tests` |
| Known folders resolve and differ | `paths::tests` |

## What is NOT verified

### 1. That any of it types Vietnamese

**Nothing here has been typed into.** No application has received a keystroke
from this build. `cargo build` proves Win32 signatures; it proves nothing about
behaviour. Every behavioural claim in Phase 4 is a hypothesis until Phase 6.

Specifically unproven: `hoongf` → `hồng`, the mid-word backspace, both boundary
re-composition cases, auto-fix and the boundary replay, and whether arrow keys
and mouse clicks flush correctly in practice.

### 2. That the tag guard works as a *behaviour*

The guard is proven as a pure function. Whether Windows actually round-trips
`dwExtraInfo` through `SendInput` → hook unchanged, on this machine, with these
flags, is unproven. **If it does not, nothing else matters** — the failure is
runaway input, not a wrong diacritic. Phase 6 Tier 1 tests it first for that
reason.

### 3. That the callback stays inside `LowLevelHooksTimeout`

Timing instrumentation is present and logs a new worst case above 10 ms. It has
never run. The hook has never been installed on a live desktop, so the actual
callback cost — and whether Windows ever removes the hook for slowness — is
unmeasured.

### 4. `ToUnicodeEx` dead-key preservation

The double-call pattern is implemented and commented. It has not been tested
against a layout that has dead keys (US-International, German). This is the
failure that would break users typing languages other than Vietnamese, and it is
invisible on a US layout — which is the layout this was written on.

### 5. Injection ordering in multiprocess applications

The macOS race (`hoongf` → `hoồng` in Chrome/Electron) was structural, not
macOS-specific. Full suppression is carried over as the fix. Whether Windows
races the same way, differently, or not at all is a Phase 6 measurement.

### 6. Whether the shipped Windows exclusion defaults are right

`windowsterminal.exe`, `pwsh.exe`, `code.exe` and the rest were written on a Mac
and have never been matched against a real foreground window. A wrong entry is
indistinguishable to a user from GlowKey being broken.

### 7. The macOS side of the Phase 0 test repair

Phase 0 rewrote six engine tests. They pass on Windows. **They have not been run
on macOS from this branch** — no Mac was available. The repair was written to be
platform-neutral rather than Windows-specific, and the fixture files are
byte-identical, but that is an argument, not evidence. The `macos-latest` CI job
is what settles it and it has not run on this branch yet.

### 8. UIPI detection against an actually-elevated window

`elevation.rs` compares integrity levels and the comparison is unit-tested. It
has not been pointed at Task Manager. Whether `OpenProcess` with
`PROCESS_QUERY_LIMITED_INFORMATION` actually succeeds against an elevated process
on this Windows build — the assumption the whole module rests on — is untested.

## What is deliberately not built

Not gaps; scope.

- **Phase 5**: tray, settings window, startup registry entry, clipboard tools,
  the four-state indicator. Elevation is *detected* and logged; it is not yet
  *shown*, which means the failure is currently invisible to a user. That is the
  single most important thing Phase 5 fixes.
- **Hotkey recording on Windows.** macOS captures a custom toggle hotkey in
  `dispatch.rs::capture_hotkey`; Windows has no equivalent yet. Existing custom
  hotkeys recorded on macOS still *match* on Windows by character, with a
  once-per-run log line saying so. Recording a new one needs the settings window,
  so it lands with Phase 5.
- **Phase 7's `windows-latest` CI job.** Unblocked now and worth landing next —
  the six Phase 0 failures survived precisely because no job ran the engine tests
  on Windows.

## The one thing to do first

Phase 6 Tier 1, item 2: confirm synthesized input is not reprocessed. If that
fails, stop — every other test is noise until injection stops re-entering the
hook.
