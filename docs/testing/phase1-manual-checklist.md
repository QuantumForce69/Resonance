# Phase 1 manual checklist

Run with `npm.cmd run tauri dev` (or the installed build). Tick each item.

## Tray & windows
- [ ] Tray icon appears at launch; tooltip says "Resonance".
- [ ] Left-click opens the flyout above the taskbar, horizontally clamped on screen.
- [ ] Clicking the tray icon again, clicking elsewhere, or pressing Esc closes the flyout.
- [ ] Flyout position is correct on a second monitor and at 125%/150% scaling.
- [ ] Main window: drag by titlebar, minimize, maximize, close → hides to tray (process keeps running).
- [ ] Tray menu: Open / Toggle microphone mute / Quit work. Launching a second copy focuses the first.

## Mixer
- [ ] Every app playing sound appears with the correct icon and friendly name; System sounds appears last.
- [ ] Volume and mute changes match the Windows volume mixer both ways (change it in Windows → Resonance updates within ~0.25 s).
- [ ] Meters move with the audio and stop when the app is silent.
- [ ] Routing an app to another output moves its sound; "Default" moves it back.
- [ ] Restart Spotify (or any app) → its saved volume/mute is restored.

## Devices
- [ ] Set default and communications devices; Windows Sound settings reflect it.
- [ ] Rename (Enter saves, Esc cancels), hide/unhide; names persist after restarting Resonance.
- [ ] Unplug/turn off the Space One → shown as Unplugged; reconnect → active again, routing restored.

## Microphone
- [ ] Level slider and mute match Windows; mic meter moves when speaking.
- [ ] Boost control appears only if the mic driver supports it and changes the Windows "Levels" boost.
- [ ] Ctrl+Shift+M toggles mute from any app, including a fullscreen game; tray icon shows the red badge.
- [ ] Push to talk: muted at rest, unmuted while held. Push to mute: the reverse.
- [ ] Recording a new hotkey works; Backspace clears it; a combo used by another app shows a readable error.

## Resilience
- [ ] Restart the "Windows Audio" service (`services.msc`) → Resonance recovers within a second.
- [ ] Sleep and wake the PC → Resonance still works.
- [ ] Corrupt `%APPDATA%\Resonance\settings.json` → app starts with defaults and a `settings.bad-*.json` backup exists.
- [ ] CPU usage with the window hidden stays near 0%.
