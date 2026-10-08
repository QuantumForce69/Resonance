# Resonance

A modern Windows audio control centre:

- per-app volume and output routing, device management
- microphone control with global hotkeys
- Soundcore Space One control (noise cancelling, EQ, battery)
- a processed microphone: AI noise suppression, gate, compressor and limiter on your mic, delivered to every
  app through the free [VB-Cable](https://vb-audio.com/Cable/) driver, with optional monitoring, and a call mix:
  send any app into your calls with its own level, with automatic ducking while you talk
- profiles: save your setup (app volumes and routing, mic processing and call sends, headphone mode and EQ,
  default output) and switch from the tray; start with Windows; a short first-run setup

Design specs and plans are in `docs/superpowers/`.

## Develop

Prerequisites: Node 20+, Rust (MSVC), Visual Studio C++ Build Tools.

    npm.cmd install
    npm.cmd run tauri dev

## Test

    npm.cmd test                                   # frontend
    cd src-tauri; cargo test --lib                 # Rust logic
    cd src-tauri; cargo test --test wasapi -- --ignored   # real-hardware checks

## Build an installer

    npm.cmd run tauri build    # → src-tauri/target/release/bundle/nsis/

Settings and logs: `%APPDATA%\Resonance`.


## Licence

GPL-3.0 (Resonance uses [OpenSCQ30](https://github.com/oppzippy/OpenSCQ30) for Soundcore headphone control). See `LICENSE`.
