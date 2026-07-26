# ARCHITECTURE

## Overview

The application is designed as a lightweight event-driven Windows utility.

Most of the application logic lives in Rust. The frontend exists only to provide a settings interface.

The application remains idle until the user presses the Hangul/English key.

---

# High-Level Architecture

```
                  Windows
                      │
                      ▼
         Low-Level Keyboard Hook
                      │
          Detect Hangul/English Key
                      │
            Selection Available?
              │               │
             No              Yes
              │               │
              ▼               ▼
      Pass Key to OS    Selection Service
                              │
                  ┌───────────┴───────────┐
                  │                       │
             UI Automation         Clipboard Fallback
                  │
                  ▼
          Selected Text
                  │
                  ▼
          Conversion Engine
                  │
                  ▼
          Replace Service
                  │
                  ▼
             Idle State
```

---

# Component Overview

The application consists of the following modules.

- Keyboard Hook
- Selection Service
- Conversion Engine
- Replace Service
- Tray Manager
- Settings
- Application Core

Each module has a single responsibility.

---

# Module Responsibilities

## Application Core

Responsible for:

- application startup
- application shutdown
- dependency initialization
- lifecycle management

The core coordinates modules but should contain little business logic.

---

## Keyboard Hook

Responsible for:

- installing the keyboard hook
- detecting the Hangul/English key
- deciding whether to intercept the key
- forwarding events to the application

The hook never performs text conversion.

---

## Selection Service

Responsible for reading selected text and capturing provider-neutral target
identity.

Preferred implementation:

1. UI Automation
2. Clipboard fallback

The rest of the application should not know which method was used.

---

## Conversion Engine

Responsible only for text conversion.

Responsibilities include:

- English → Korean conversion
- Korean → English conversion
- Hangul composition
- Hangul decomposition
- exact preservation of line endings, whitespace, indentation, and all
  non-convertible characters

The conversion engine must remain platform independent.
It processes input as a character stream and flushes Hangul composition at
every non-convertible character, copying that boundary character verbatim.

It should have no dependency on:

- Windows APIs
- Clipboard
- UI Automation
- Keyboard hooks

---

## Replace Service

Responsible for replacing the current selection with converted text.

The replacement implementation may vary depending on the platform APIs used.

---

## Input Language Service

The Windows-specific Input Language Service reads and synchronizes the input
language of the focused control and GUI thread captured with the original
selection. It inspects the keyboard layout and Korean IME conversion state,
changes them only when necessary, and revalidates the captured target before
every mutation. It never blindly simulates another Hangul/English key press.
For the Korean keyboard layout, Hangul/English mode is read and changed through
the target input context's IME open status and verified after mutation.
If a modern Windows IME ignores the direct setter, the service revalidates the
captured target and conditionally sends one marked Hangul toggle only while a
verified mismatch remains, then verifies the resulting state.

This service is invoked only after replacement succeeds. A synchronization
error is reported as partial success: the converted text remains in place and
replacement is neither undone nor repeated. The platform-independent
Conversion Engine has no dependency on this service or Windows IME APIs.

---

## Tray Manager

Responsible for:

- tray icon
- tray menu
- opening settings
- pause/resume
- application exit

---

## Settings

The Settings Store is the single source of truth for strongly typed
configuration. During startup it loads and validates the local JSON file,
recovers to defaults when the file is missing or corrupt, reconciles Windows
startup registration, and writes through a temporary file plus atomic replace.

The Tauri command boundary exposes settings values and update/reset operations;
the frontend never receives filesystem access. Successful updates are persisted
before the in-memory snapshot changes. Startup registration is performed
through a platform boundary and a failed registration rejects that update so
the stored value remains consistent.

Runtime consumers receive immutable snapshots through a shared runtime view and
an observer channel. The dispatcher applies the conversion enable switch and
hotkey mode before starting any selection work. Provider preferences are passed
into selection and replacement operations rather than manipulated by the
Settings module. This keeps configuration ownership separate from hook,
service, and conversion behavior.

---

# Event Flow

The normal workflow is:

```
User

↓

Select Text

↓

Press Hangul Key

↓

Keyboard Hook

↓

Selection Service

↓

Conversion Engine

↓

Replace Service

↓

Continue Editing
```

If no selection exists:

```
User

↓

Press Hangul Key

↓

Keyboard Hook

↓

Forward Event

↓

Windows Default Behavior
```

---

# Dependency Rules

Dependencies must flow in one direction.

```
Core
 │
 ├── Hook
 ├── Tray
 ├── Settings
 └── Services
      │
      ├── Selection
      ├── Replace
      └── Converter
```

The Conversion Engine must be completely independent from platform-specific modules.

Modules should communicate through interfaces whenever possible.

---

# Folder Structure

```
src/
    Settings UI (React)

src-tauri/
    core/
    hook/
    selection/
    converter/
    replace/
    tray/
    settings/
```

The Rust backend contains all application logic.

The frontend only provides the settings interface.

---

# Performance Goals

The application should:

- remain idle most of the time
- use event-driven execution
- avoid polling
- minimize memory usage
- respond immediately after the Hangul/English key is pressed

The utility should feel invisible during normal computer usage.

---

# Conversion Operation State

The application coordinator owns one conversion operation at a time:

```text
Idle
  -> ReadingSelection
  -> Converting
  -> ValidatingTarget
  -> Replacing
  -> RestoringClipboard
  -> SynchronizingInputLanguage
  -> Completed
  -> Idle
```

Every success and failure path uses a scope guard to return to `Idle`.
Concurrent activations are rejected while another operation is active.

Selection results include a provider-neutral target snapshot containing the
foreground window, focused control, process, and GUI thread identity.
Replacement validates that identity again. The clipboard replacement provider
also copies and compares the current selection with the original snapshot
immediately before pasting. When UI Automation and clipboard APIs represent
the same newline with different encodings, validation compares canonical line
boundaries and the replacement adopts the clipboard selection's exact original
CR, LF, or CRLF sequence.

UI Automation selection and replacement calls have bounded worker deadlines.
Clipboard locking, access retries, simulated copy, paste settling, and
restoration also use bounded deadlines. Timeout and target-change outcomes are
returned as structured categories rather than blocking the hook thread.

The coordinator maps English-to-Korean conversion to Korean input mode and
Korean-to-English conversion to English input mode. If synchronization fails
after replacement, the outcome remains handled so the original Hangul key is
not replayed against already-converted text.

Clipboard transactions are serialized. They capture all enumerable formats,
track the clipboard sequence number after each utility-owned write, and restore
only while that sequence is still current. If another application or the user
changes the clipboard, the newer contents win and the saved snapshot is not
restored over them.

Synthetic input carries a private marker that the low-level hook ignores.
Copy and paste injection rejects conflicting Shift, Alt, or Windows modifiers,
respects an already-held Control key, and releases only keys introduced by the
utility after a partial injection.

The Windows hook recognizes `VK_HANGUL`/`VK_KANA` (`0x15`) and the Korean
Hangul-key scan-code form `0x72`. Only key-down starts an operation; key-up,
auto-repeat, held-key repeats, and utility-injected input do not.

Operation errors are consolidated into privacy-safe categories including no
selection, unsupported target, target changed, clipboard busy or externally
changed, selection/replacement timeout, UI Automation failure, replacement or
conversion failure, concurrent operation, and internal failure. Diagnostics
log lifecycle, provider choice, and categories without selected or clipboard
contents.

---

# Future Extensions

The architecture should allow future additions without changing the core conversion engine.

Possible extensions include:

- additional keyboard layouts
- user-defined conversion rules
- application-specific behavior
- advanced shortcut customization

These features should be implemented as separate modules whenever possible.
