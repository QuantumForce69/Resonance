# Resonance

A modern Windows audio control centre: per-app volume and output routing, device management,
microphone control with global hotkeys — with Soundcore Space One control and a Voicemeeter-style
engine coming in later phases (see `docs/superpowers/specs/`).

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
