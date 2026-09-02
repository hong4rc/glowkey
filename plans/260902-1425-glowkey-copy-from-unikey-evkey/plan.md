---
title: "GlowKey — features worth copying from Unikey / EVKey"
status: completed
created: 2026-09-02
branch: main
---

# What GlowKey can copy from Unikey / EVKey

Full audit of Unikey (Windows) and EVKey (macOS) features against GlowKey's
current state, with an honest have / copy / skip verdict for each. Only the
"copy — worthwhile" items become phases; the rest are recorded so the decision is
visible, not lost.

## Audit — every notable Unikey/EVKey feature

| Feature | GlowKey today | Verdict |
|---|---|---|
| Telex input | ✅ | Have |
| **VNI input** (`viet65`→việt) | ✗ — Telex only | **Copy — Phase 1** (vi crate has `vi::VNI`) |
| VIQR input | ✗ | Skip — near-unused today |
| Free tone marking (bỏ dấu tự do) | ✅ `hoongf`/`hofong` | Have |
| Modern/classic tone placement (oà/òa) | ✅ tone style | Have |
| Immediate diacritics (`oo`→ô) | ✅ | Have |
| Spell check / restore invalid word | ✅ auto-fix (`exit` not `eĩt`) | Have |
| Remove tone with `z` + re-edit | ✅ recomposition (`hồng␣⌫z`→hông) | Have |
| **Auto-capitalize first letter of sentence** | ✗ | **Copy — Phase 2** (small toggle) |
| Per-app / per-window language memory | ✅ per-app exclusions | Have (stronger than EVKey) |
| Quick toggle hotkey | ✅ ⌃⇧Space / ⌃⇧E | Have |
| **Configurable toggle hotkey** | ✗ — fixed | **Copy — Phase 3** |
| Menu-bar icon showing V/E | ✅ VI/EN glyph | Have |
| On-toggle notification | ✅ HUD | Have |
| Run at login | ✅ login item (menu + Settings) | Have |
| Show control panel on start | ✅ open-Settings-on-launch | Have |
| **About / version window** | ✗ | **Copy — Phase 4** (trivial, expected) |
| Macros / quick-type (gõ tắt) | ✗ | Defer — large feature, its own plan |
| Legacy encodings (TCVN3, VNI-Windows, Unicode tổ hợp) | ✗ Unicode NFC only | Skip — every modern macOS app is Unicode |
| Clipboard encoding conversion | ✗ | Skip — tied to legacy encodings |
| Beep/sound on toggle | ✗ | Skip — low value, HUD already confirms |
| Auto-update | ✗ | Out of scope for this plan |

## Phases (only the worthwhile copies)
| # | Phase | Value | Effort | Status |
|---|-------|-------|--------|--------|
| 1 | [VNI input method](./phase-01-vni-input-method.md) | High | ~2h | ✅ done |
| 2 | [Auto-capitalize sentence](./phase-02-auto-capitalize.md) | Medium | ~2h | ✅ done |
| 3 | [Configurable toggle hotkey](./phase-03-configurable-hotkey.md) | Medium | ~3h | ✅ done |
| 4 | [About window](./phase-04-about-window.md) | Low | ~1h | ✅ done |

## Acceptance criteria
- A Settings "Input method" control switches Telex ⇄ VNI; typing follows it; persists.
- Optional "Auto-capitalize first letter" produces `Xin chào` from `xin chaof`… (sentence start).
- The toggle hotkey is user-settable (at least a small preset list); persists.
- An About window shows name, version, and a one-line credit.
- All engine changes test-covered; clippy clean; bundle builds.

## Non-goals (explicit skips, with reason)
- Legacy encodings / clipboard conversion — modern macOS is all Unicode NFC.
- VIQR input, beep-on-toggle — negligible real-world demand.
- Macros (gõ tắt) — genuinely useful but a large feature; separate plan if wanted.

## Recommendation
Do **Phase 1 (VNI)** first — highest value, lowest risk (a definition swap the
`vi` crate already supports), and the one real input gap vs Unikey/EVKey. Phases
2–4 are nice-to-haves; pick per appetite. Skip the encoding features outright.
