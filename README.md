# CC-22

A boutique modular multi-FX plugin: four colour engines you chain in any order,
each with five hand-tuned modes, plus a 10-band independent EQ rack with a live
input spectrum behind the curve.

> VST3 · CLAP · Standalone — Windows (x86-64).

## Modules & modes

| Module | Modes |
|--------|-------|
| **Character** | Drive · Sweeten · Fuzz · Howl · Swell |
| **Movement** | Doubler · Vibrato · Phaser · Tremolo · Pitch |
| **Diffusion** | Cascade · Reels · Space · Collage · Reverse |
| **Texture** | Filter · Squash · Cassette · Broken · Interference |

Each module has its own **Pre** and **Post** EQ, plus a **Global Pre** and
**Global Post** EQ — ten fully independent equalizers. The signal chain is
re-orderable; per-module EQs follow their module.

```
Input → Global Pre EQ → [ Module Pre → Module → Module Post ] × chain order → Global Post EQ → Output
```

## System requirements

- Windows 10/11 (64-bit)
- A VST3 or CLAP host, or run the standalone

## Install

**VST3** — copy `CC-22.vst3` to:

```
C:\Program Files\Common Files\VST3\
```

**CLAP** — copy `CC-22.clap` to:

```
C:\Program Files\Common Files\CLAP\
```

**Standalone** — run `CC-22.exe`. If the audio backend reports a buffer-size
mismatch, launch with a matching period, e.g. `CC-22.exe --period-size 1056`.

## Build from source

Requires a recent stable Rust toolchain.

```sh
cargo test                                    # run the test suite
cargo run --release --package xtask -- bundle cc_22 --release
```

Bundles are written to `target/bundled/`.

## License

CC-22 is proprietary software. See [EULA.md](EULA.md).

© 2026 Rafa Audio. All rights reserved.
