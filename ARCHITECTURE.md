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

Responsible for reading and replacing selected text.

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

The conversion engine must remain platform independent.

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

## Tray Manager

Responsible for:

- tray icon
- tray menu
- opening settings
- pause/resume
- application exit

---

## Settings

Responsible only for configuration.

Examples:

- startup option
- hotkey
- update preferences

Settings must not contain business logic.

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

# Future Extensions

The architecture should allow future additions without changing the core conversion engine.

Possible extensions include:

- additional keyboard layouts
- user-defined conversion rules
- application-specific behavior
- advanced shortcut customization

These features should be implemented as separate modules whenever possible.