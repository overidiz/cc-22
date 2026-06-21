# CC-22

A boutique modular multi-FX plugin: four colour engines you chain in any order,
each with five hand-tuned modes, plus a 10-band independent EQ rack with a live
input spectrum behind the curve.

> VST3 · CLAP · Standalone — Windows (x86-64).

## The 20 official modes

| Module | Modes |
|--------|-------|
| **Character** | Drive · Sweeten · Fuzz · Howl · Swell |
| **Movement** | Doubler · Vibrato · Phaser · Tremolo · Pitch |
| **Diffusion** | Cascade · Reels · Space · Collage · Reverse |
| **Texture** | Filter · Squash · Cassette · Broken · Interference |

Each module is one slot in the chain and exposes exactly these five modes — no
hidden or legacy modes. Modules start **bypassed** by default.

## Signal flow

```
Input Gain
  → Global Pre EQ
  → for each module in chain order:
        Module Pre EQ → Module → Module Post EQ
  → Global Post EQ
  → Output Gain
  → Global Dry/Wet
  → Global Bypass
  → Output
```

The four modules (Character, Movement, Diffusion, Texture) are **re-orderable** —
drag them into any order. The two Global EQs stay fixed at the ends; the
per-module EQs travel with their module when the chain is reordered.

## The EQ system — 10 independent EQs

CC-22 has **ten** fully independent equalizers, each with its own parameters,
stable parameter IDs, and DSP state (an `EqRack` of 10 instances):

- **Global Pre EQ** — shapes the signal before any module.
- **Global Post EQ** — shapes the final output. (Legacy single-EQ state migrates
  here.)
- **Character / Movement / Diffusion / Texture — Pre & Post** — a dedicated EQ
  immediately before and after each module.

In the UI, one full-width curve shows the **selected** EQ. Pick which one with:

- **Target**: Global / Character / Movement / Diffusion / Texture
- **Position**: Pre / Post
- **Band**: B1–B5

Edit directly on the graph: **drag a node** for frequency + gain, **scroll** on it
for Q, **right-click** for the band type (Off disables it), **double-click** to
reset the band. **Reset** only affects the currently selected EQ. A live **input
spectrum analyzer** is drawn behind the curve so you can shape against the signal.

Because every EQ is a separate instance, changing one never touches another:
global pre vs. post, per-module pre vs. post, and one module's EQ vs. another's
are all isolated (verified by tests).

## Install

**VST3** — copy `CC-22.vst3` to:
```
C:\Program Files\Common Files\VST3\
```

**CLAP** — copy `CC-22.clap` to:
```
C:\Program Files\Common Files\CLAP\
```

**Standalone** — run `CC-22.exe`. It auto-detects your audio device's buffer
size, so it launches with no arguments.

> Or just run the installer: **`CC-22-1.0.0-Setup.exe`** (installs all three +
> a desktop shortcut and an uninstaller).

## Migration from older projects

Earlier CC-22 builds exposed extra modes that have since been consolidated into
the 20 official ones. Projects saved with an old build are migrated automatically
on load (via `Plugin::filter_state`):

- Removed modes map onto their closest surviving mode (e.g. Character *Clean* →
  *Sweeten*, *Saturation* → *Drive*; Diffusion *Delay/Slap* → *Reels*, *Reverb* →
  *Space*; Texture *Wow-Flutter/Tape* → *Cassette*, *Noise* → *Broken*).
- An old dedicated **Off** mode maps onto the module's first mode **and engages
  the module bypass**, so a migrated project never suddenly applies an effect.
- The legacy single global EQ migrates into **Global Post EQ**.
- Chain order, surviving modes, and EQ parameters are preserved untouched.

No legacy enum variants remain in the runtime — migration happens purely at the
saved-state level.

## Build from source

Requires a recent stable Rust toolchain.

```sh
cargo test
cargo run --release --package xtask -- bundle cc_22 --release
```

Bundles are written to `target/bundled/`.

## License

CC-22 is proprietary software. See [EULA.md](EULA.md).

© 2026 Rafa Audio. All rights reserved.
