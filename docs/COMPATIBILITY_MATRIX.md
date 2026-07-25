# Phase 6 compatibility matrix

Status values: **Supported**, **Supported through fallback**, **Unsupported**,
or **Known limitation**.

This table records current generic-provider results. Cases that require an
interactive application session are explicitly marked as known limitations
until that manual run is completed on the target Windows installation.

| Application / control | English to Korean | Korean to English | Mixed | No selection | Clipboard | Repeated / rapid | Focus change |
|---|---|---|---|---|---|---|---|
| Windows Notepad | Known limitation (manual run pending) | Known limitation (manual run pending) | Known limitation (manual run pending) | Known limitation (manual run pending) | Known limitation (manual run pending) | Known limitation (manual run pending) | Known limitation (manual run pending) |
| VS Code editor | Supported through fallback | Supported through fallback | Supported through fallback | Supported through fallback | Supported through fallback | Known limitation (manual run pending) | Supported |
| Chrome/Edge single-line input | Supported through fallback | Supported through fallback | Supported through fallback | Supported through fallback | Supported through fallback | Known limitation (manual run pending) | Supported |
| Browser multiline textarea | Supported through fallback | Supported through fallback | Supported through fallback | Supported through fallback | Supported through fallback | Known limitation (manual run pending) | Supported |
| Browser contenteditable | Supported through fallback | Supported through fallback | Supported through fallback | Supported through fallback | Supported through fallback | Known limitation (manual run pending) | Supported |
| Windows Search/native field | Known limitation (manual run pending) | Known limitation (manual run pending) | Known limitation (manual run pending) | Known limitation (manual run pending) | Known limitation (manual run pending) | Known limitation (manual run pending) | Supported |

"Supported" in the focus-change column means the operation aborts with
`TargetChanged`; it does not paste into the newly focused control.

## Procedure for each row

1. Convert the key sequence `dkssudgktpdy` to its composed Korean text.
2. Convert compatibility-Jamo input for `hello` to `hello`.
3. Convert a mixed Korean/English selection as one unit.
4. Press the key with no selection and confirm normal Windows behavior.
5. Preload rich clipboard data, convert, and confirm every original format is
   still available. Change the clipboard during a delayed operation and confirm
   the newer data is retained.
6. Repeat conversions and rapid key presses; confirm one conversion per
   physical key press and continued responsiveness.
7. Move focus while conversion is running; confirm neither the old nor new
   control is modified.

## Generic limitations

- Elevated targets can reject input from a non-elevated HanYeongKey process.
- Generic UI Automation selection ranges are read-only. Direct replacement is
  used only when the selected text is verified to cover a writable control's
  entire ValuePattern; partial selections use the clipboard fallback.
- Applications that block synthetic copy/paste, do not expose selection through
  UI Automation, or do not place text on the clipboard are reported as
  unsupported or temporarily unavailable without replacement.
