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


## Phase 2 — Space One
- [ ] Headphones tab shows the Space One as connected with battery, firmware and serial.
- [ ] Mode lever switches noise cancelling / transparency / normal, and you hear the change.
- [ ] Manual noise-cancelling levels 1–5 and transparency levels 1–5 apply; Adaptive shows its current level.
- [ ] Wind-noise reduction toggles.
- [ ] Choosing an EQ preset changes the sound; moving a fader switches the picker to Custom and the sound follows smoothly.
- [ ] Pressing the headphones' own ANC button updates Resonance within about a second.
- [ ] Turning the headphones off shows "Not connected, retrying…"; turning them on reconnects without clicking Retry.
- [ ] The tray flyout strip shows battery and switches mode.

## Phase 3a — Processed mic (needs VB-Cable from https://vb-audio.com/Cable/)

- [ ] Without VB-Cable: Microphone page shows the install message; "Get VB-Cable" opens the download page; everything else works.
- [ ] Turn Processing on: Windows Sound settings show "CABLE Output" as default and communications mic; the flyout mic key shows "Processed".
- [ ] Discord/Teams voice test with "Default" input: voice is clear, fan/keyboard noise suppressed, no echo, no crackle over 10 minutes.
- [ ] Gate: silence between words is quiet; raising the threshold cuts more; Advanced Hold/Release behave.
- [ ] Compressor: loud and soft speech end up closer in level; Amount 0 vs 100 is clearly different; Auto makeup toggles.
- [ ] Limiter: shouting never clips in a recording (Audacity on CABLE Output).
- [ ] Monitoring: "Hear yourself" plays the processed mic in the headphones; level slider works; off by default.
- [ ] Mute key/shortcut mutes what apps hear while processing is on.
- [ ] Unplug the source mic: status shows "Paused" with a reason; plug it back in: processing resumes within a few seconds.
- [ ] Turn Processing off: the previous default and communications mics come back.
- [ ] Quit from the tray while processing: previous mics come back; relaunch: processing turns itself back on.
- [ ] Kill Resonance in Task Manager while processing, relaunch, turn Processing off: the original mics come back.
- [ ] Change the default mic in Windows while processing is on, then turn Processing off: your manual choice is kept.

## Phase 3b — Call mix (needs Processing on)

- [ ] Mixer: every app (not System sounds) has a Call key; with Processing off it's dimmed and explains why.
- [ ] Send Spotify: the key lights, a Send knob appears; in a Discord voice test the other side hears the music and you.
- [ ] You still hear Spotify on your own speakers/headphones, unchanged.
- [ ] Send level and "Apps in call" both change how loud the music is in the call.
- [ ] Ducking: the music drops while you talk and comes back about half a second after you stop; "Duck by" changes how much; turning ducking off keeps it steady.
- [ ] Chrome with several tabs playing: heard once, not doubled.
- [ ] Close Spotify while sent, reopen it and play: it's heard in the call again without touching anything.
- [ ] Sending Discord shows the echo warning.
- [ ] Turn Processing off and on: sends come back. Quit and relaunch: sends are remembered.
- [ ] Loud music never clips in a recording of CABLE Output.

## Phase 4 — Profiles, Start with Windows, first-run setup

- [ ] First launch after installing shows the welcome; Skip or Done closes it for good.
- [ ] Save "Gaming" (Apps + Headphones) with the game loud and ANC on; save "Work" (Apps + Headphones) with music low and transparency on.
- [ ] Switching from the tray flyout restores the volumes and the headphone mode; the active button is lit.
- [ ] Save "Call" (Mic & processing): processing on, Spotify sent; switching to it turns processing on and sends Spotify.
- [ ] A profile with only Default output leaves app volumes alone.
- [ ] With the Space One off, switching to Gaming applies the apps and says the headphones were skipped.
- [ ] An app that wasn't running gets its saved volume and output when it starts.
- [ ] Update, Rename and Delete (with confirmation) work; deleting the active profile clears the highlight.
- [ ] Start with Windows on: sign out and in → Resonance is in the tray, no window. Off: it doesn't start.
