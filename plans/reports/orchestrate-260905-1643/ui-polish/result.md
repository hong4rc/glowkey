# GlowKey UI polish audit — 2026-09-05

Code-and-spec review. No app launched. **VERIFIED** = read in source. **SUSPECTED** = needs eyes on screen.
Owner column: **spec** = `app/src/settings_spec.rs` (or `strings`/`prefs_model`), **mac** = `app/src/prefs/*`, `menu_bar.rs`, `hud.rs`, `welcome.rs`, `about_window.rs`, **win** = `app/src/platform/windows/*`, **doc** = `docs/ui-design.md`.

Read: `docs/ui-design.md`, `settings_spec.rs`, `prefs/{mod,tabs,widgets,excluded,macros_window,personal_words}.rs`, `platform/windows/{settings_ui,theme,tray,indicator,about_ui,ui_thread,shell}.rs`, `menu_bar.rs`, `hud.rs`, `welcome.rs`, `about_window.rs`, `strings.rs`, `prefs_model.rs`, both manual-verification docs, `GlowKey_Brand_Guidelines.pdf`, `GlowKey_Assets/`.

---

## P0 — user cannot tell what the app is doing, or cannot complete a core task

### P0-1. Windows: adding an excluded app is a free-text box asking for an .exe filename
`platform/windows/settings_ui.rs:1020-1033` — `TextEdit::singleline`, hint `t("program name","tên chương trình")`, fed to `normalize_exe_name` (`:1470`, lowercases and trims, validates nothing). `:1049` renders each row as the raw id string. macOS gets `NSOpenPanel` over `/Applications` and resolves the bundle id for you (`prefs/mod.rs:277-302`), with icon + Finder name in the list (`prefs/excluded.rs:143-196`).
VERIFIED. A user must know that Slack is `slack.exe`, Word is `winword.exe`, IntelliJ is `idea64.exe`. Typo → a row that looks added and never fires. This is the headline feature with a hostile input surface, and it is asymmetric with macOS.
Owner: **win**. Fix: replace the text field with a picker of *currently running, visible-window processes* (name + exe + icon from `EnumWindows`/`GetWindowThreadProcessId`/`ExtractIconEx` — `foreground.rs` already resolves exe names for the hook), plus a "Browse…" `IFileOpenDialog` filtered to `*.exe`, keeping the text field as an escape hatch. Show the friendly name in list rows with the exe underneath, so a stale entry is legible. Minimum viable, if the picker is too big: list running processes in a combo. Verify: unit test on the enumeration→id mapping headless; human eyes for the picker.

### P0-2. Windows: no HUD, so a hotkey toggle produces no on-screen feedback
`hud.rs:129` is the only `flash`, and it is `objc2`; only caller is `platform/macos/dispatch.rs:188`. Nothing on the Windows side flashes. VERIFIED.
Consequence: `Ctrl+Shift+Space` changes a 16px tray glyph the user is not looking at. `Ctrl+Shift+E` (per-app) changes it to *dimmed VI*, a difference of ink at 16px in the corner of the screen — on Windows the tray icon may also be hidden in the overflow flyout, in which case the toggle is completely silent. macOS additionally flashes the auto-fix correction (`"was → ứa"`, `hud.rs:96-99`); Windows never explains a correction.
Owner: **win**. Fix: a borderless, click-through, topmost, 0.7 s layered window (or a fourth egui viewport with `with_decorations(false).with_taskbar(false).with_always_on_top()`, already the shape of the `ui_thread` shim at `ui_thread.rs:110-115`) showing the same three text bands `hud.rs` shows. Verify: headless test that the toggle path calls `flash`; human eyes for placement/DPI.

### P0-3. Windows: no first-run guide at all, so both extra hotkeys are undiscoverable
`prefs_model.rs:86-87` `welcome_shown` is a real field; `platform/windows/shell.rs:343-346` merges it; nothing on Windows ever reads it to show anything (`grep welcome` finds no Windows caller). `welcome.rs` is `NSAlert`, macOS-only. The Windows tray menu has no "Quick Guide" item (`tray.rs` cmd list `:47-60`, menu body `:719-838`).
VERIFIED. On Windows a new user gets a tray icon and no explanation of `Ctrl+Shift+Space`, `Ctrl+Shift+E` or `Ctrl+Shift+W`, and no way to ask for one.
Owner: **win** (+ **spec**: lift the welcome copy out of `welcome.rs` into a shared `Text` block so it cannot drift). Fix: a small egui viewport (same host as About) shown once when `!welcome_shown`, plus a "Quick Guide…"/"Hướng dẫn nhanh…" tray item. Verify: headless draw test like `about_ui.rs:136`; human eyes once.

### P0-4. Windows: Settings changes do not apply until the window is closed
`settings_ui.rs:1397-1399` — `finalize()` only on `close_requested`; `take_result` (`:797`) is the only way a value leaves. Every checkbox, the input method, tone marks, the hotkey and all three list editors write `self.draft` and nothing else. Only `Control::Language` applies live (`:887-889`).
VERIFIED. `docs/ui-design.md:111` states the contract: *"No Done/OK button anywhere: every change applies live and is saved at once."* macOS honours it (`prefs/mod.rs` handlers call `state()` setters directly, e.g. `:208-217`). On Windows a user unticks "Auto-fix", types in Notepad, sees auto-fix still firing, and concludes the setting is broken. There is also no Done button telling them a commit is pending — the window looks live and is not.
Owner: **win**. Fix: push each edit to the session as it happens (the merge in `shell.rs` already exists; call it per change instead of once at close), keeping `finalize` as the safety net. If a live path is genuinely out of reach this release, the window must grow an explicit **Apply/Close** affordance and `ui-design.md:111` must record the exception — silently deferring is the one option that is not acceptable.
Verify: headless test that a draft edit produces a session write before close.

### P0-5. Windows tray menu never states the current state
`tray.rs:722` — top item is `"GlowKey"` with the logo bitmap. macOS puts the state first: `menu_bar.rs:261-268`, `"Vietnamese"` / `"English"` / `"Excluded in {app}"` as a disabled header, exactly as `ui-design.md:46-58` sketches. On Windows the state exists only in the tooltip (`tray.rs:396`), which requires a deliberate hover, and in the checkmarks on two separate items the user must read together (`:756-778`).
VERIFIED. The single question this app must answer at a glance — *is Vietnamese on, and is it on here* — has no plain-language answer in the Windows menu.
Owner: **win**. Fix: replace `"GlowKey"` with `indicator.describe(app)` as the disabled first line (it already produces exactly the right sentence for all five states, `indicator.rs:119-163`), and keep the logo bitmap on it. Verify: headless assertion that the first menu string equals `describe`; human eyes for truncation of the long elevated string in a popup menu.

---

## P1 — looks unfinished, or fights the platform

### P1-1. Vietnamese: "Auto-fix" and "Restore English words" both become *khôi phục*
`settings_spec.rs:372-374` — `"Auto-fix non-Vietnamese words"` → `"Tự động khôi phục từ không phải tiếng Việt"`; `:390-391` — `"Restore common English words"` → `"Khôi phục từ tiếng Anh thông dụng"`. English distinguishes *auto-fix* from *restore*; Vietnamese uses `khôi phục` for both, and the section headers (`Tự sửa` :368, `Từ tiếng Anh` :388) do not rescue it. VERIFIED.
Owner: **spec**. Fix: `"Tự sửa từ không phải tiếng Việt"` for `AutoFix` (matches its own section header `Tự sửa`), leave `Khôi phục…` to `RestoreEnglishWords`. Verify: read.

### P1-2. Two names for one feature: "Excluded apps" vs "ignore list" / *loại trừ* vs *bỏ qua*
`settings_spec.rs:410` `"Excluded apps"` / `"Ứng dụng loại trừ"`; `indicator.rs:124-131` tooltip says `"(ignore list)"` / `"(danh sách bỏ qua)"`; `prefs/excluded.rs:1-7` and `menu_bar.rs` call it the ignore list in comments and *Tắt cho "X"* in UI; Windows tray says *Gõ tiếng Việt trong {app}*. Four vocabularies for one concept, in both languages. VERIFIED.
Owner: **spec** (own the noun) + **mac**/**win** (adopt it). Fix: pick one — recommend EN **"excluded apps"**, VI **"ứng dụng loại trừ"** — and make `indicator.rs` say it. Verify: grep test asserting the tooltip contains the spec's noun.

### P1-3. Vietnamese section header and row label are the identical string, stacked
`settings_spec.rs:341` section `Method`/**`Kiểu gõ`** immediately above `:343` row `Input method`/**`Kiểu gõ`**. Same at `:417` section `Macros`/**`Gõ tắt`** above `:420` row `Macros`/**`Gõ tắt`**. English has the same problem in the Macros case. VERIFIED — the Vietnamese Typing tab literally reads "Kiểu gõ / Kiểu gõ [Telex|VNI|Telex đơn giản]".
Owner: **spec**. Fix: section `Kiểu gõ` + row label `Kiểu` is wrong; better section `Cách gõ`/`Typing` with row `Kiểu gõ`; for Macros, drop the row label (the section header carries it) or rename the section `Gõ tắt & mở rộng`. Verify: add a spec test — no row label may equal its section title in either language.

### P1-4. The AutoFix setting is called three different things across surfaces
Settings: `"Auto-fix non-Vietnamese words"` (`settings_spec.rs:372`). macOS menu: `"Auto-fix English words"` / `"Tự động sửa từ tiếng Anh"` (`menu_bar.rs:297`). Windows tray: the same English-words wording (`tray.rs:787`). And there is a *separate* setting actually about English words (`RestoreEnglishWords`). VERIFIED.
Owner: **spec** (add a `Text` for the menu/tray line beside the row label) + **mac**/**win** (consume it). Verify: test that the tray/menu string is the spec's.

### P1-5. macOS Settings tabs do not scroll, and the window is not resizable
`prefs/tabs.rs:59-61` style mask is `Titled|Closable|Miniaturizable` — no `Resizable`; `:93` `window.setContentView(tabs)` puts the `NSTabView` straight in, each tab a bare `NSStackView` (`widgets.rs:24-39`) with no `NSScrollView`. `WINDOW_SIZE` is fixed at 460×540 (`:33`).
This is the exact defect `ui-design.md:129-134` and `widgets.rs:78-90` describe fixing in the three list windows — unfixed in the main window. The Corrections tab is 5 rows, 4 wrapped captions; Vietnamese runs ~20 % longer than English (`settings_spec.rs:392-394` is a 2-line English caption); at larger system text sizes it will overflow 540 pt with no way to reach the bottom rows. Windows has both a scroll area (`settings_ui.rs:1431-1436`) and a resizable window (`:56-57`).
VERIFIED (absence of scroll view). Overflow itself SUSPECTED — needs a Mac at Vietnamese + large text.
Owner: **mac**. Fix: add `NSWindowStyleMask::Resizable` and wrap each tab stack in the existing `scrollable()` helper. Verify: human eyes at 460 pt / Vietnamese / accessibility text size.

### P1-6. Broken-state glyph is `⚠` on macOS and `!` on Windows
`menu_bar.rs:210-211` `"⚠"`; `indicator.rs:100` `"!"`; `ui-design.md:36` specifies `⚠`. VERIFIED. `!` in a 16px tray slot reads as a stray punctuation mark, not a warning; the brand set has no broken glyph either.
Owner: **win** + **doc**. Fix: draw `⚠` (Segoe UI Symbol covers it) or a real triangle path in `draw_glyph` (`tray.rs:473-565` already paints with GDI, so a path is cheap and DPI-clean); update `docs/manual-verification-windows.md:129-131` which currently codifies `!`.

### P1-7. `open_settings_at_launch` defaults to **true**
`prefs_model.rs:106`. Combined with launch-at-login this puts a Settings window in the user's face on every login. `ui-design.md:19` states the design value: *simple, native, **unobtrusive***; `welcome.rs:10-14` argues explicitly that first-run should be a guide, not the Settings window. VERIFIED.
Owner: **spec/prefs_model**. Fix: default `false`; keep the checkbox for people who want it. (First-run discovery is P0-3's job, not this.)

### P1-8. Neither renderer names the control a screen reader lands on
macOS: `tabs.rs:311-313` sets `accessibilityHelp` from the caption — good — but the row's label `NSTextField` is never linked to the control (`accessibilityTitleUIElement` / `accessibilityLabel` are not set anywhere in `prefs/`). VoiceOver on the tone picker announces the segment titles, not "Tone marks".
Windows: `settings_ui.rs:319-321` reports `WidgetInfo::selected(SelectableLabel, …, &current)` — the *value*, never the row label; and captions are drawn as plain labels (`finish_row` `:559-566`), so Narrator gets none of the explanatory text macOS gives VoiceOver.
VERIFIED (absence). Owner: **mac** + **win**. Fix: mac — `setAccessibilityLabel:` from `row.label` on every control built in `add_row`; win — include the label in `WidgetInfo` and attach the caption via `Response::on_hover_text` + an AccessKit description. Verify: `settings_ui.rs:1955` already has a screen-reader test to extend headlessly; VoiceOver needs a Mac.

### P1-9. Windows: keyboard navigation through the segmented controls is untested and structurally fragile
`settings_ui.rs:323-357` locks horizontal arrows to the track and moves the selection; `docs/manual-verification-windows.md:177-180` records this as **not run**. Two specific risks, both SUSPECTED: (a) the tab strip is itself a `segmented` (`:1416-1421`), so ←/→ *changes tab* whenever focus is anywhere on it — plausible, but it also means there is no Ctrl+Tab / Ctrl+PgUp-PgDn, which is the Windows idiom for tabs; (b) `ui.next_auto_id()` + `skip_ahead_auto_ids(1)` (`:305-306`) ties focus identity to draw order, and the label column is re-measured per frame (`:825`), so a language switch mid-frame can reshuffle ids and drop focus.
Owner: **win**. Fix: add Ctrl+Tab / Ctrl+Shift+Tab on the tab strip; give each segmented control a stable `id_salt` derived from the row's `Toggle`/`Control`, not the auto-id sequence. Verify: extend the existing headless arrow tests (`:1854`, `:1890`) with a language switch between frames; human eyes for the Tab ring.

### P1-10. `Ctrl+Shift+W` / `⌃⇧W` appears in exactly one caption and nowhere else
`settings_spec.rs:399-400` — the Personal words caption, on the Corrections tab. Not in the welcome guide (`welcome.rs:69-82` names only the toggle and `⌃⇧E`), not in either menu, not in About. VERIFIED.
Owner: **spec** + **mac**/**win**. Fix: add it to the welcome copy as a third line ("`{fix_word}` — sửa từ vừa gõ và ghi nhớ"), and add a `Control::Shortcut(Shortcut::FixWord)` row to the General → Keyboard section beside `ToggleApp` (`:327-335`) — that section exists to list the fixed shortcuts and currently lists one of two.

### P1-11. `⌃⇧E` is not shown on the menu item that performs it
macOS shows the configurable toggle's spelling on its item (`menu_bar.rs:290-292`) but the per-app item `"Disable for “X”"` (`:272-277`) carries no shortcut hint, though `⌃⇧E` does exactly that. Windows' equivalent (`tray.rs:766-778`) shows neither shortcut. VERIFIED.
Owner: **mac** + **win**. Fix: append the spelling from `shortcut_display(Shortcut::ToggleApp)` to the item title (a title suffix, not a key equivalent — the hotkey is owned by the tap/hook, as `menu_bar.rs:281-283` notes). Windows: also append `hotkey_display` to *Vietnamese input*, which macOS already does.

### P1-12. macOS: `Add App…` accepts any file and fails silently
`prefs/mod.rs:279-301` — `setCanChooseFiles(true)` with no `allowedContentTypes`, so a `.txt` is selectable; if `bundleIdentifier()` is `None` the loop simply skips it (`:294-299`) and `refresh_list()` shows nothing new. VERIFIED.
Owner: **mac**. Fix: `setAllowedContentTypes:` to `UTTypeApplicationBundle`, and `notify()` (the alert helper already exists, `:463`) when a chosen item yields no bundle id.

### P1-13. macOS: excluded-apps caption is hand-written, hard-broken, and duplicates the spec
`prefs/excluded.rs:59-65` — contains literal `\n` line breaks and a hardcoded `⌃⇧E`, both of which `settings_spec.rs` exists to prevent (`:57-60`; the spec's own test `every_text_has_both_languages_and_no_hard_breaks` at `:597` would reject this string). Windows' equivalent intro is a *third* wording (`settings_ui.rs:1007-1017`). Three copies of one paragraph. VERIFIED.
Owner: **spec** (add an intro `Text` per `ListId`) + both renderers.

### P1-14. Windows: a duplicate macro is silently overwritten; macOS asks
`settings_ui.rs:1586` (`upsert_macro_replaces_case_insensitive_duplicate_instead_of_adding`) — no prompt. macOS runs an alert with Replace/Cancel (`prefs/mod.rs:394-399`) and a three-way Keep/Replace/Cancel on import (`macros_window.rs:289-291`). VERIFIED.
Owner: **win**. Fix: a confirm step, or at minimum an inline "replaced *vn*" line under the row. Also: the Windows table import (`settings_ui.rs:1210-1217`) reports nothing at all, while macOS reports "Imported N macros / N kept as they were" (`macros_window.rs:317-320`) — importing 214 macros into the Windows box currently looks like nothing happened until you scroll the list.

### P1-15. macOS About has no icon and no Copy; `ui-design.md` says it does
`about_window.rs:46-118` — no image view, no Copy button, no `NSImageView` import. `ui-design.md:114-116` claims *"About mirrors the macOS window: icon, name, version with commit and a Copy"*; the Windows About actually has all three (`about_ui.rs:54-69`). So Windows is ahead and the doc describes Windows as if it were macOS. VERIFIED.
Owner: **mac** (add the icon + Copy) + **doc** (correct the claim). Brand note: `GlowKey_Brand_Guidelines.pdf` p.1 says the wordmark is "set as a lockup for README headers and **the About window**" — neither About uses `GlowKey_Assets/wordmark.svg`.

### P1-16. Windows: no confirmation on any destructive list action
`settings_ui.rs:1050`, `:1166`, `:1302` — Remove takes effect immediately with no undo and no confirm, in three windows. macOS is the same for Remove (`excluded.rs:181-193`), so this is a *shared* gap rather than a parity one, but on Windows it compounds with P0-4: the removal is not even persisted until the Settings window closes, so "did that work?" has no answer on screen.
Owner: **both renderers**. Fix: cheapest correct thing is an inline Undo affordance after a removal, not a modal.

### P1-17. Windows checkbox titles do not wrap
`settings_ui.rs:573-580` — `ui.checkbox(value, label)` inside a `horizontal`, at `column + COLUMN_GAP` (~110–130 px in) in a 460 pt window, with a vertical-only `ScrollArea` and `auto_shrink([false,false])` (`:1431-1434`) so overflow is clipped, not scrollable. macOS explicitly measures and wraps the same string (`prefs/tabs.rs:347-376`, whose doc comment names *"Tự động khôi phục từ không phải tiếng Việt"* as the one that forced it).
SUSPECTED — arithmetic puts it within a few points of the right edge at 100 % scale, over it at a wider label column or larger system text. Owner: **win**. Fix: `ui.add(Checkbox::new(value, RichText::new(label)).wrap())` or allocate the remaining width and wrap. Verify: headless — lay out the longest Vietnamese title at the real column width and assert it fits; human eyes at 125 %/150 % DPI.

---

## P2 — refinement

### P2-1. Mixed Vietnamese orthography inside one window
`Xóa` (classic placement) at `prefs/excluded.rs:184`, `settings_ui.rs:1050/1166/1302`; `Huỷ` (modern placement) at `prefs/mod.rs:399` and `settings_ui.rs:1144`. The app ships a *Modern hoà / Classic hòa* switch (`settings_spec.rs:300-309`) — its own chrome should be internally consistent. VERIFIED.
Owner: **spec**/both renderers. Pick one — recommend modern (`Xoá`, `Huỷ`), matching the app's own default `PlacementStyle::New`. Add a spec test if the strings move into `settings_spec.rs`.

### P2-2. Vietnamese label wording, three specific ones
- `settings_spec.rs:328` `"Toggle Vietnamese"` → **`Chuyển tiếng Việt`**. *Chuyển* reads as "switch/convert/translate". Prefer **`Bật/tắt tiếng Việt`**.
- `:330` `"Toggle current app"` → **`Bật tắt ứng dụng này`** reads as "turn this app on/off", i.e. as if it toggles the application. Prefer **`Bật/tắt trong ứng dụng hiện tại`**. Missing slash in `Bật tắt` throughout is also non-standard.
- `:352` `"Quick Telex"` → **`Gõ tắt phụ âm`** collides with *gõ tắt* = macros (`:182`, `:418`). Two unrelated features share a name in Vietnamese. Prefer **`Telex nhanh`**, the term UniKey users already know.
VERIFIED. Owner: **spec**.

### P2-3. "Input method" has no caption; "Simple Telex" is undefined
`settings_spec.rs:343` — the one row where a user must choose between three named systems has no explanation, while `Quick Telex` (`:350-355`) — far less consequential — has a good one. VERIFIED. Owner: **spec**. Fix: one sentence naming what Simple Telex drops.

### P2-4. Hotkey row: Windows has no recorder and does not say so
`settings_ui.rs:935-955` offers presets only; `hotkey_choices` (`:257-266`) drops `Alt+Space` and appends a foreign saved value. macOS adds a "Custom…" segment with a live recorder and a status line (`prefs/tabs.rs:241-261`, `mod.rs:113-137`). A user who set a custom combo on a Mac sees an unexplained fifth entry; a Windows user has no way to record one and no caption saying why. VERIFIED. Owner: **win** (+ **spec** for a caption). Fix minimum: a caption. Better: a recorder — the hook already sees every key.

### P2-5. Two renderers, two metaphors for the per-app switch
macOS: an action verb whose label flips — `"Disable for “X”"` / `"Enable for “X”"` (`menu_bar.rs:272-276`). Windows: a state checkbox — `"Vietnamese in {app}"` with `MF_CHECKED` (`tray.rs:766-778`). Both are defensible; only one should be GlowKey's. VERIFIED. Owner: **spec** (own the string) + both. Recommend the checked-state form: it also states the current state, which is what P0-5 is about.

### P2-6. Menu content diverges beyond the two platform idioms
macOS has `Reset input (if stuck)` (`menu_bar.rs:316-323`) and `Quick Guide…` (`:371-378`); Windows has neither (`tray.rs` cmd list `:47-60`). Windows has `Reinstall the keyboard hook` (`:745-752`), correctly platform-specific. VERIFIED. Owner: **win**. Fix: add both (Quick Guide is P0-3; Reset is one call).

### P2-7. Section-header rhythm differs by 2 pt
`settings_ui.rs:654` adds `ui.add_space(2.0)` after a header; `prefs/tabs.rs:123` adds the header to the stack and lets the stack's own 6 pt spacing follow, with no extra. So header→first-row is 8 px on Windows, 6 pt on macOS. The 6/10/18 rhythm claimed in `ui-design.md:120-122` otherwise checks out exactly (`tabs.rs:46-47` vs `settings_ui.rs:103-105`, both 6/10/18, both with the checkbox in the control column and both with `{count} {unit}` from `ListId::unit`). VERIFIED — parity is real apart from this. Owner: **win** or **mac**, pick one.

### P2-8. Brand assets are unused, and under-specify the product
`GlowKey_Assets/menu_bar_{on,off}.svg` are template silhouettes for **two** states; the product has **four** (`ui-design.md:30-37`), and neither renderer loads them — macOS sets a text title (`menu_bar.rs:220`), Windows draws text with GDI (`tray.rs:473-565`). The guidelines' ON glyph carries `ẳ`-style diacritics that will not survive 16 px on a 100 % Windows taskbar. `ui-design.md:202-203` leaves this as an open question. VERIFIED.
Recommendation: keep text glyphs (they are the only thing that reads at 16 px and they scale with the menu-bar font), and either retire the two SVGs or extend the brand set to four states. Do not ship a two-state icon for a four-state indicator.

### P2-9. Empty and error states, per window
Present and good: excluded/macros/words empty lines on both platforms (`prefs/excluded.rs:136-140`, `macros_window.rs:144-145`, `personal_words.rs:148-149`; `settings_ui.rs:1042`, `:1156`, `:1291`), macOS import/export errors (`macros_window.rs:217-357`), the Windows tombstone section (`settings_ui.rs:1064-1097` — a genuinely nice touch macOS lacks entirely).
Missing, VERIFIED: (a) the *tombstone/restore* affordance exists only on Windows — on macOS a removed shipped default is unrecoverable through the UI; (b) Windows macro import reports nothing (P1-14); (c) `install_system_font` returns a bool that says "this window can draw Vietnamese" (`settings_ui.rs:131-157`) — if it is `false` the Vietnamese UI is a wall of tofu and nothing tells the user or falls back to English; (d) macOS `Add App…` silent failure (P1-12).
Owner: as noted. For (c): if the font did not load, force `Language::English` for the session and log it.

### P2-10. Windows: no Mica/backdrop, custom greys instead of system colours
`settings_ui.rs:205-228` hardcodes gray 236 / gray 40 to imitate macOS, and `raise_controls` (`:233-250`) paints macOS-style push buttons. `theme.rs` correctly reads `AppsUseLightTheme` and `SystemUsesLightTheme` separately — that part is right and well argued. But the window is a macOS window rendered on Windows: no Mica backdrop, no Windows 11 accent colour, no rounded-corner/DWM attributes, and controls that do not match any other Settings dialog on the machine. `ui-design.md:12-17` claims "a menu-bar tool should look like it came with the system"; on Windows it looks like it came with a Mac. SUSPECTED on visual impact (needs eyes); VERIFIED as a code fact.
Owner: **win**. Fix (cheap, high return): use the system accent colour for selection/focus (`DwmGetColorizationColor` or `UISettings::GetColorValue`) and let the segmented "raised" fill follow it, rather than white. Mica via `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE)` is optional and fights egui's opaque panel fill — not worth it this release.

### P2-11. Font scaling / high-DPI
Windows: sizes are fixed points in `apply_style` (`:175-181`) and the viewport is specified in points (`:54`, with the comment claiming winit's scale factor handles it) — so the window scales with display DPI but **not** with the user's "Make text bigger" accessibility slider, which is a separate Windows setting egui does not read. macOS: system font at fixed 11 pt captions (`tabs.rs:418`, `widgets.rs:49`) rather than `NSFont::preferredFontForTextStyle`, so Dynamic Type does not apply either — while `ui-design.md:186` claims *"Reduced motion / Dynamic Type: … system font scales."* That claim is false on both. VERIFIED. Owner: **doc** (correct the claim) + both renderers (P2, real work).

---

## Verified-correct, for the record

Not everything needs changing, and these were checked rather than assumed:

- The 6/10/18 rhythm, the checkbox-in-control-column decision, `{count} {unit}`, the dependent-row indent+disable, and the label-column measurement are genuinely parallel across the two renderers (`prefs/tabs.rs:36-47,207-240,381-394,476-496` ↔ `settings_ui.rs:88-105,573-581,615-631,842-867`). The handoff's parity claim holds for these.
- `settings_spec.rs`'s test block is unusually good: one-home-per-setting (`:629`), dependent-after-parent (`:669`), caption length (`:616`), no hard breaks (`:597`), placeholder expansion (`:713`). Most of the spec-level findings above are things those tests *could* be extended to catch.
- `indicator.rs:73-91` — severity ordering, and `Reach::Unknown` deliberately not reporting broken, is the right call and well defended.
- `theme.rs` — reading the two registry values separately, and defaulting to light, is correct.
- macOS list windows are resizable + scrolled (`excluded.rs:26-32,84`); the Windows list viewports likewise (`settings_ui.rs:68-80`, `:1372-1379`). The main window is the outlier (P1-5).

---

## Suggested order

1. P0-4 (deferred apply) and P0-2 (no HUD) — both are "the app did not respond" bugs.
2. P0-1 (exe text field) — the headline feature.
3. P0-3 + P1-10 + P1-11 (discovery of all three hotkeys) — one copy pass, three surfaces.
4. P0-5, P1-6 (Windows tray truth-telling).
5. The spec/Vietnamese copy pass: P1-1, P1-2, P1-3, P1-4, P1-13, P2-1, P2-2, P2-3 — one edit to `settings_spec.rs`, lands on both platforms, verifiable headlessly.
6. P1-5, P1-8, P1-9, P1-17 (layout and accessibility).
7. P2s.

## Unresolved questions

1. **P0-4**: is a live-apply path on Windows actually available this release, or does the hook/session ownership model force the close-time merge? If forced, the window needs a visible Apply and `ui-design.md:111` needs an exception recorded.
2. **P0-1**: is a running-process picker acceptable scope, or should this be `IFileOpenDialog` over `*.exe` only? The former matches how people think about it; the latter is an afternoon.
3. **P1-7**: is `open_settings_at_launch: true` a deliberate onboarding decision that P0-3 would replace, or an oversight?
4. **P2-8**: keep the text glyph and retire the brand's two-state icon set, or commission four states? This closes `ui-design.md` open question 2 either way.
5. **P2-5**: which per-app metaphor is GlowKey's — the macOS action verb or the Windows checked state?
