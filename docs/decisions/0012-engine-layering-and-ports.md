# 0012 — Three library crates, and a port the shells implement

## Status

Accepted (2026-09-05).

## Context

Until this change `glowkey-engine` was one crate holding the Vietnamese
transformation, the typing policy built on it (VN/EN mode, the per-application
ignore list, auto-fix, macros, word overrides), the product's preferences file,
and the per-platform tables of applications that ship excluded. A consumer who
wanted only the transformation got serde, a settings schema, a list of macOS
bundle identifiers and Windows executable names, and a hotkey preset type. The
two shells each carried out the ladder's `Decision` and `Effects` in their own
code, so the contract a third shell would have to meet existed only as two
implementations to read side by side.

Plan: `plans/260905-1333-engine-split-and-layering/`. Validation decisions are
recorded in its `plan.md`.

## Decision

Four layers, each a crate or a directory, each depending only on the one below:

| Layer | Owns | Knows nothing about |
|---|---|---|
| `glowkey-engine` | `Engine`, `KeyResponse`, `InputMethod`, `PlacementStyle`, tone removal, the spell check (`is_invalid_vietnamese`) and `diff` | applications, modes, files, platforms |
| `glowkey-session` | `Session` (facade) and `SessionBuilder`, `ExclusionList` with injected `ExclusionDefaults`, `AppId`, `Macro`, `WordOverride`; re-exports the engine | what an application is called, which ones ship excluded, any OS |
| `glowkey-input` | the decision ladder (`decide`), hotkey matching, and the port: `trait Platform` plus `handle` | any OS |
| `app/` | `Settings` and its file, `Language`, the shipped exclusion tables (`default_exclusions/`), one `Platform` implementation per shell | — |

Patterns, named so they are recognisable rather than for their own sake:

- **Facade**: `Engine` and `Session` remain the two entry points; the module
  split behind them changed no signature.
- **Builder**: `Session::builder()` for the dozen policy knobs the preferences
  adapter sets.
- **Dependency injection**: `ExclusionList::with_defaults(ExclusionDefaults)`
  and `from_saved(.., defaults)`. The defaults carry both tables, excluded and
  terminals, because the terminal rule (a hotkey only *suspends* a terminal's
  exclusion) is toggle semantics, not just data. The constructor makes every
  terminal a default so the two tables cannot disagree.
- **Newtype**: `AppId` for the frontmost application. Opaque: the session
  compares and looks up, never parses.
- **Ports and adapters**: `Platform` has six methods, all of which both shipping
  shells already did: `inject`, `replay_key`, `app_in_front`, `request_save`,
  `request_indicator`, and `notify`, the one loosely typed channel for what a
  shell shows or logs (`Notice`). `handle` runs `decide` and then the port in a
  fixed order: the `Decided` notice, the `Effects` in field order, the decision.
  The Windows adapter (`HookPort`) acts immediately, since injection is
  `SendInput` and the rest are flags plus a thread message. The macOS adapter
  (`TapPort`) *queues* edits and the replay and `handle_key_down` posts them
  after the policy returns, so that the tests can still call `decide` with real
  `CGEvent`s on a developer's machine without typing into it.
- **Strategy** (pre-existing, kept): input methods are `vi::methods::Definition`
  values chosen by `InputMethod`.

`serde` is an optional feature on the engine and the session crate. The default
engine build depends on `vi` and `phf` only.

## Consequences

- A consumer who wants Vietnamese typing takes `glowkey-engine`. One who wants
  an input method's behaviour takes `glowkey-session`, builds a session, tells it
  which application is in front, feeds it keys. One who wants GlowKey's whole
  keyboard policy takes `glowkey-input`, implements `Platform`, and calls
  `handle`.
- The settings file is byte-compatible: `Settings` moved crates with its serde
  shape intact, and the exclusion merge rule (`saved ∪ (defaults − removed)`)
  moved with its tests.
- The log line order on macOS changed slightly: the `KEY` line now precedes the
  `TOGGLE mode` line, as it always did on Windows. Both shells now write the same
  `Decision` text through its `Display`.
- Anything a new shell needs that the port lacks is either a bug in the port or
  a platform fact; the trait grows only for the first.
- Nothing in the three library crates names a platform. The Linux CI job builds
  and tests all three, with and without `serde`, and that is what keeps it so.

## Alternatives rejected

- **A `Platform` trait passed into `decide` itself.** The ladder is pure data in,
  data out, and its tests depend on that; a callback inside it is an operating
  system waiting to happen. The port sits after the decision, not inside it.
- **One `glowkey-core` crate with feature flags for session and policy.**
  Features are not layers: a consumer can still see every name, and a wrong
  `use` compiles. Crates make the dependency direction a build error.
- **Keeping the exclusion tables in the session crate under `cfg(target_os)`.**
  That was the one `cfg` in the engine and it was already a `cfg` too many for a
  crate that claims to know no OS. The tables are the product's; they live with
  it.
- **Baking `is_terminal` as a free function over a static table.** It is what
  made the tables unmovable. As a method on the list's injected defaults it
  travels with the list.
