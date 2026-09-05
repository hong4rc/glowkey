# Phase 2 — Privacy and the log

**Depends on:** phase 1 (shares `log.rs`).
**Gated by:** decision A2 (is full-text logging load-bearing for debugging?) —
only for the toggle. The documentation half is unconditional and should ship
first.

## The problem, stated plainly

GlowKey writes what you type to a file, always, with no way to turn it off.

- **macOS** (`dispatch.rs:276-279`): the whole composing word, as
  `raw="hoo" rendered="hô"`.
- **Windows** (`hook.rs:498-506`): the character per key, plus
  `Corrected {was}->{becomes}`.
- **When:** every key-down that reaches `handle` (`platform.rs:118-122`),
  whatever the decision — *including in excluded apps*, which `PRIVACY.md:35`
  admits.
- **Gate:** none. `log.rs:134-137` gates only on `cfg!(test)`; `GLOWKEY_DEBUG`
  controls the stderr echo, not the file.
- **Permissions:** `log.rs:95-99` opens with `create().append()` and no
  `set_permissions` anywhere. On macOS `~/Library` is 0700 and protects it
  incidentally; on Windows nothing does.
- **Volume:** up to 10 MB of attributed typing at rest (5 MB × 2 generations).

Any process running as the user can read it. That is the ordinary
post-compromise position, and for a keyboard tool it is the whole ballgame.

**And the disclosure is wrong.** `PRIVACY.md` is macOS-only prose throughout. It
never names the Windows log path. Its "nothing you type ever leaves your Mac" has
no Windows counterpart — and on Windows it would be false as written, because
settings live in `FOLDERID_RoamingAppData` (`paths.rs:17-18`, whose own comment
says "roam with the user's profile"), so **macros — verbatim text the user
typed — roam off the machine** on a managed profile.

The secure-field exemption is macOS-only too: `mod.rs:31` and `PRIVACY.md:36`
describe a guarantee that comes from the OS withholding secure input from event
taps. A `WH_KEYBOARD_LL` hook has no such exemption, and nothing in the Windows
code implements one.

## Work

### 2.1 — Fix the disclosure (unconditional, do first)

Rewrite `PRIVACY.md` platform-neutrally: both log paths, both settings paths, the
roaming caveat, what a log line actually contains, and the explicit statement
that Windows has **no** secure-field exemption. Remove "your Mac" as the subject.

This is a documentation change, but it is the highest-priority item in the phase:
the current file tells Windows users something untrue about their own machine.

### 2.2 — Let the user delete it (unconditional)

A "Delete log" item in both the macOS menu and the Windows tray, next to the
existing reveal item. Today the only way to remove a 10 MB record of your typing
is to find it in a folder.

### 2.3 — Restrict permissions (unconditional)

Explicit `0600` on the log and settings files on Unix; a restrictive DACL on
Windows. Currently protection is incidental on one platform and absent on the
other.

### 2.4 — Redact by default (gated on decision A2)

Recommended shape: the default log records the *decision*, counts and timings —
everything §7 of the handoff uses for triage — without characters. An opt-in
**diagnostic typing log** restores `raw=`/`rendered=` and expires automatically.

This preserves the debugging workflow the project genuinely depends on while
making the default safe. It is deliberately a setting, not a build flag: bug
reports come from users, who must be able to turn it on and back off.

## Validation

- Headless: a test asserting no character content appears in a log line in the
  default mode.
- Read `PRIVACY.md` against the code paths listed above, line by line — every
  claim must name the platform it applies to.

## Risk

2.4 changes what the maintainer sees when diagnosing. Ship 2.1–2.3 first; do not
let 2.4's decision hold up the disclosure fix.
