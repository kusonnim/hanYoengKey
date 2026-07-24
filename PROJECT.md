# PROJECT

## Overview

This project is a lightweight Windows background utility that fixes mistyped Korean and English keyboard input.

Instead of requiring users to copy text into a separate application, the utility integrates naturally into the existing Windows text editing workflow.

The user simply selects text and presses the Hangul/English key (한/영). The utility converts the selected text and replaces it immediately.

The application should feel like a built-in Windows feature rather than a standalone application.

---

## Goals

The primary goals are:

- Instantly convert selected text between Korean and English keyboard layouts.
- Preserve the user's existing workflow.
- Require no additional copy/paste steps.
- Run silently in the background.
- Consume minimal CPU and memory.
- Feel indistinguishable from a native Windows feature.

---

## User Experience

The expected workflow is:

1. Select text.
2. Press the Hangul/English key.
3. The selected text is immediately replaced with the converted version.
4. Continue typing normally.

Example:

```
dkssudgktpdy
↓

안녕하세요
```

and

```
ㅗ디ㅣㅐ
↓

hello
```

If no text is selected, the Hangul/English key must behave exactly as Windows normally does.

---

## Core Features

### Text Conversion

- Convert English keyboard input into Korean.
- Convert Korean keyboard input into English.
- Support full Hangul composition.
- Support mixed Korean and English text without heuristics.

Mixed text is converted uniformly according to the selected conversion direction.

Example:

```
안녕 hello 테스트

↓

dkssud hello xptmx
```

Running the conversion again produces:

```
안녕 ㅗ디ㅣㅐ 테스트
```

---

### Background Operation

The application should:

- Start automatically with Windows (optional setting).
- Run in the system tray.
- Not appear in the taskbar.
- Not appear in Alt+Tab.
- Operate continuously with minimal resource usage.

---

### Settings

The application provides a lightweight settings window accessible from the system tray.

Typical settings include:

- Launch at startup
- Hotkey configuration
- Pause / Resume
- About
- Exit

---

## Technical Stack

- Rust
- Tauri 2
- React
- TypeScript

Rust contains the application logic.

React is used only for the settings interface.

---

## Design Principles

The project follows these principles:

- Native Windows experience.
- Fast startup.
- Low memory usage.
- Event-driven architecture.
- Modular components.
- Clear separation of responsibilities.
- Platform-independent conversion engine.

---

## Non-Goals

This project does not aim to:

- Become a full Input Method Editor (IME).
- Replace the Windows keyboard layout system.
- Modify keyboard behavior during normal typing.
- Continuously monitor or alter user input.
- Collect or transmit user data.

The utility only operates when the user explicitly selects text and invokes conversion.