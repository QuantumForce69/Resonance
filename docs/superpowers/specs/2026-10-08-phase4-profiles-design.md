# Resonance Phase 4 — Profiles, start with Windows, first-run setup

**Date:** 2026-10-08
**Status:** Design approved in brainstorming; awaiting spec review
**Parent spec:** `2026-10-06-resonance-design.md` (Phase 4). Narrowed to what the user wants: profiles switched
from the tray flyout (no hotkeys, no auto-switching), start with Windows, and a first-run setup.

## 1. Goal

Switch between setups like "Gaming", "Work" and "Call" in one click: each profile restores just the parts of
Resonance it was saved with. Resonance can start with Windows, and new installs get a short guided setup.

## 2. Scope

**In scope**
- Profiles: save the current setup with chosen parts, apply, update (re-capture), rename, delete.
- Switching from buttons in the tray flyout and from a new Profiles tab.
- Start with Windows (minimised to the tray).
- First-run setup (once).

**Out of scope (YAGNI)**
Profile hotkeys; automatic switching when apps launch or close; "modified since applied" tracking; importing or
exporting profiles; per-profile mic hotkey settings.

## 3. Approach

Profiles are partial snapshots stored in `settings.json` and applied through the same code paths the existing
controls use (`AppCore` for audio, the headset manager for the Space One, `win/startup.rs` for start-up).
Rejected: swapping whole settings files (no partial profiles, can't drive the headphones, clobbers unrelated
preferences); one file per profile (no benefit).

## 4. What a profile holds

A profile has an id, a name and up to four **parts**; a part that's absent is left alone when applying.

| Part | Captured | Applied |
|---|---|---|
| **Apps** | For each app known to Resonance (playing now or remembered): volume, mute, output device (or "default") | Running apps change at once. Volume and mute are written to the remembered app preferences, so apps that aren't running get them when they next play. Output devices for apps that aren't running are applied when they next play (kept in memory until then). |
| **Mic & processing** | The whole processing settings (on/off, source, every stage, monitoring, Call mix master and ducking) and the call sends (which apps, at what level) | Replaces both, exactly as if set by hand (default-mic switching, engine start/stop and sources follow). |
| **Headphones** | Ambient mode, noise-cancelling mode, manual NC level, transparency level, wind-noise suppression, EQ preset or custom bands | Sent as headset commands in that order. If the Space One isn't connected, the part is skipped with a notice. |
| **Default output** | The default playback device (console and communications roles) | Set as the default for both roles. If it's unplugged, skipped with a notice. |

The default mic isn't a part: processing manages it.

A profile can't be saved with no parts. The Headphones part can only be ticked while the Space One is connected.

## 5. Switching and management

- **Tray flyout:** a row of profile buttons above the master output (scrolls horizontally when there are many). The
  last applied profile is lit. Clicking applies it.
- **Profiles tab** (main window, after Headphones): one card per profile with its name, part chips, and Apply,
  Update, Rename, Delete (Delete asks for confirmation). A "Save current setup" form: name + part checkboxes.
- **Update** re-captures the profile's existing parts from the current setup.
- The last applied profile id is saved, so the highlight survives restarts. It doesn't apply on start-up.

## 6. Start with Windows

- Settings tab switch "Start with Windows": a `Resonance.lnk` shortcut in the user's Startup folder
  (`shell:startup`) that runs Resonance with `--minimized`. (First built with `tauri-plugin-autostart`'s registry
  Run entry, which Windows skipped at sign-in on the user's PC; release builds migrate that entry to the shortcut.)
- Launched with `--minimized`, Resonance stays in the tray regardless of the "Start minimised" setting.
- The switch reads the real state (whether the shortcut exists). Release builds re-point an existing shortcut at
  themselves on launch.

## 7. First-run setup

- Shown over the main window when `settings.ui.onboarded` is false; finishing or skipping sets it true.
- Steps: **Microphone** (choose the default mic), **VB-Cable** (installed ✓, or explanation + "Get VB-Cable"),
  **Finish** (switches: "Turn on Processing" if VB-Cable is installed, "Start with Windows"; then Done).
- Not shown in the flyout. Existing installs see it once (their settings predate the flag).

## 8. Error handling

- Applying a profile never stops at the first problem: every part and item is attempted; each failure becomes one
  notice (toast), e.g. "Speakers isn't connected, so the default output wasn't changed".
- Applying a profile whose saved apps include ones no longer installed is harmless (they're remembered for later).
- Deleting the active profile clears the highlight.

## 9. Testing

- **Controller (fake backend/engine):** saving captures only ticked parts; applying changes only those parts; app
  values reach running apps and remembered preferences; pending outputs apply when an app appears; processing and
  call sends are replaced; an unplugged default output becomes a notice; update/rename/delete; active profile kept
  and cleared on delete; at least one part required.
- **Headset part:** state → commands mapping (pure), skipped with a notice when disconnected.
- **Frontend:** Profiles tab (save form, cards, actions, delete confirmation), flyout buttons with the active one
  lit, Settings start-with-Windows switch, first-run steps (VB-Cable present/missing, skip, finish).
- **Hardware (`#[ignore]`):** save a profile with Apps + Default output, change things, apply, and read back volumes
  and the default output from Windows; restore afterwards.
