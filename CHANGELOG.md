# Changelog

All notable changes to CC-22 are documented here.

## [1.0.0] — 2026-06-21

First release.

### Effects
- Four modules, five official modes each (20 total):
  - **Character**: Drive, Sweeten, Fuzz, Howl, Swell
  - **Movement**: Doubler, Vibrato, Phaser, Tremolo, Pitch
  - **Diffusion**: Cascade, Reels, Space, Collage, Reverse
  - **Texture**: Filter, Squash, Cassette, Broken, Interference
- Re-orderable signal chain with deterministic repair of corrupt saved orders.
- Modules start bypassed by default.

### EQ
- Ten fully independent EQs: global pre/post + pre/post for every module, each
  with its own parameters, IDs, and DSP state.
- Full-width EQ curve edited directly on the graph (drag = freq/gain, scroll = Q,
  right-click = band type); side knob panel removed.
- Live input **spectrum analyzer** behind the curve (lock-free capture, UI-thread
  FFT).

### DSP safety
- Denormal flushing, clamped feedback, soft-clip safety limiters, DC blocking.
- No NaN/Inf/panic in production code paths.
- Reports a processing tail so hosts keep delay/reverb tails alive.

### Compatibility & packaging
- VST3, CLAP, and standalone builds.
- Standalone auto-detects the audio device's WASAPI buffer size (no more
  buffer-mismatch crash; launches with no arguments).
- Legacy state migration: removed modes map onto the 20 official ones.
- Windows installer (Inno Setup) for VST3 + CLAP + standalone.

### Validation
- pluginval strictness 8: pass (multi-sample-rate, state restoration, automation,
  thread safety).
- clap-validator: pass (state reproducibility).
- 353 automated tests; 0 compiler warnings.

[1.0.0]: https://github.com/overidiz/cc-22/releases/tag/v1.0.0
