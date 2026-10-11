# Resonance patches to openscq30-lib

Vendored from https://github.com/oppzippy/OpenSCQ30 tag v2.12.0 (commit 3c77383).
Only lib, lib-has, lib-macros, i18n and i18n-macros are kept; the workspace `members`
in `Cargo.toml` and the unused `[patch]` sections were trimmed accordingly.
Resonance uses it through `[patch."https://github.com/oppzippy/OpenSCQ30"]` in `src-tauri/Cargo.toml`.

## Changes

1. `lib/src/devices/soundcore/a3035/structures.rs`: byte 2 of the [6,129] sound modes body is
   always written as 0. Upstream wrote the ambient sound mode there. Soundcore Space One firmware
   05.87 always reports 0 and silently ignores commands where it is nonzero (ACKs them, mode never
   changes). It also stores whatever it is sent, so echoing the reported byte could keep a bad
   value; writing 0 recovers from that too. Untested on other firmware versions.
2. `lib/src/devices/soundcore/common/modules/sound_modes_v2/packet_handler.rs`: the inbound
   [6,1] handler passed `new == old` to `send_if_modified`, so change notifications fired only
   when nothing changed. Now `new != old`. Physical button presses therefore update the UI.
   This handler is shared by every sound_modes_v2 device (A3035, A3040, A3062, A3936, A3947),
   and now matches the v1 handler (`common/modules/sound_modes/packet_handler.rs`).
3. `lib/src/devices/soundcore/common/device.rs`: test-only `TestSoundcoreDevice::send_inbound`.
4. `lib/src/devices/soundcore/a3035.rs`: updated `set_custom_transparency` expectations and
   added tests for byte 2 (including a stale nonzero value) and for [6,1] notifications
   firing on a change and not on an identical state.
