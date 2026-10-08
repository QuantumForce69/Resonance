# Resonance

A modern Windows audio control centre:

- per-app volume and output routing, device management
- microphone control with global hotkeys
- Soundcore Space One control (noise cancelling, EQ, battery)
- a processed microphone: AI noise suppression, gate, compressor and limiter on your mic, delivered to every
  app through the free [VB-Cable](https://vb-audio.com/Cable/) driver, with optional monitoring

Stream mixes and profiles come in later phases (see `docs/superpowers/specs/`).

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
