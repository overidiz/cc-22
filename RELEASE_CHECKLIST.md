# CC-22 — Release Checklist (v1.0.0)

A step-by-step from "code done" to "selling it". Items marked **[done]** are
already complete in this build; the rest are yours.

---

## 1. Code & build health — **[done]**
- [x] `cargo fmt` clean
- [x] `cargo check --all-targets` — 0 warnings, 0 errors
- [x] `cargo test` — 353 passing, 0 failing
- [x] `cargo build` + `cargo run --release --package xtask -- bundle cc_22 --release`
- [x] No `unwrap/expect/panic` in production code paths
- [x] Version is `1.0.0` in `Cargo.toml`
- [x] Real metadata (vendor, URL, email, CLAP id, VST3 class id)

Re-run before any new release build:
```sh
cargo fmt && cargo check --all-targets && cargo test
cargo run --release --package xtask -- bundle cc_22 --release
```

## 2. Format validation — **[done]**
- [x] **VST3** passes `pluginval` strictness 8
- [x] **CLAP** passes `clap-validator` (15/0)

Re-run after changes:
```sh
C:\Users\Rafa\tools\pluginval\pluginval.exe --strictness-level 8 --validate ".\target\bundled\CC-22.vst3"
C:\Users\Rafa\tools\clap-validator\clap-validator.exe validate ".\target\bundled\CC-22.clap"
```

## 3. Manual DAW testing — **[YOU — do this first]**
Test the installed VST3/CLAP in at least two hosts (e.g. Reaper + one of
Ableton/FL/Bitwig). For each:
- [ ] Plugin scans and loads without error
- [ ] Audio passes; all **20 modes** sound correct and distinct
- [ ] All **10 EQs** (global pre/post + per-module pre/post) work independently
- [ ] Chain reorder works; per-module EQs follow their module
- [ ] Global bypass + per-module bypass work
- [ ] Automate a few params — no zipper/clicks
- [ ] **Save project → close → reopen → state restored exactly**
- [ ] Switch presets; no crashes
- [ ] High-feedback settings (Reels/Reverse/Space) don't blow up
- [ ] CPU usage acceptable on a full chain
- [ ] No crashes over a 15–30 min session

> If a mode sounds bad: note *which mode* + *what's wrong* (harsh? quiet? clicks?)
> and it can be fixed with a concrete target.

## 4. Code signing — **[YOU — needed for a paid product]**
Unsigned binaries trigger Windows SmartScreen "unknown publisher" warnings.
- [ ] Buy an **Authenticode / OV code-signing certificate** (DigiCert, Sectigo,
      SSL.com — ~US$70–250/yr; EV certs avoid SmartScreen reputation warm-up)
- [ ] Sign the plugin binaries **and** the installer:
```sh
signtool sign /fd SHA256 /tr http://timestamp.sectigo.com /td SHA256 ^
  ".\target\bundled\CC-22.vst3\Contents\x86_64-win\CC-22.vst3" ^
  ".\target\bundled\CC-22.clap" ^
  ".\target\bundled\CC-22.exe"
:: rebuild the installer AFTER signing the payload, then sign the installer:
signtool sign /fd SHA256 /tr http://timestamp.sectigo.com /td SHA256 ^
  ".\installer\output\CC-22-1.0.0-Setup.exe"
```
- [ ] Verify: `signtool verify /pa ".\installer\output\CC-22-1.0.0-Setup.exe"`

> Inno Setup can sign automatically via a `[Setup] SignTool=` directive once a
> sign tool is configured in the Inno IDE.

## 5. Legal & repo — **[YOU]**
- [ ] Have a lawyer review **`EULA.md`** (jurisdiction, refunds, consumer law)
- [ ] Decide repo visibility: **make `overidiz/cc-22` private** (it is public now,
      so anyone can rebuild/resell the source) — or accept it being open
- [ ] Confirm support email is correct (currently `rafatoledoreis@gmail.com`)

## 6. Packaging the download — **[mostly done]**
- [x] Installer builds: `installer\output\CC-22-1.0.0-Setup.exe`
- [x] Installer verified end-to-end (installs VST3 + CLAP + standalone + shortcut,
      registers uninstaller)
- [ ] (Optional) Also ship a plain `.zip` for users who prefer manual install
- [ ] Sign the installer (see step 4)

## 7. Store / distribution — **[YOU]**
- [ ] Pick a store: **Gumroad** (simplest), Lemon Squeezy, or your own site
- [ ] Set a price
- [ ] Product page: name, 2–3 line hook, feature list, **screenshots** (use the
      ones already captured on the Desktop), system requirements (Windows 10/11,
      VST3/CLAP host), demo video optional
- [ ] Upload the signed installer
- [ ] (Optional) license-key system — boutique plugins often ship without DRM

## 8. Post-release
- [ ] Tag the release in git (`git tag v1.0.0 && git push --tags`)
- [ ] Keep a `CHANGELOG.md`
- [ ] Plan: macOS build (≈half the market) is the biggest growth item for v1.1

---

### Right-now priority order
1. **DAW testing (step 3)** — gates everything.
2. Legal + repo privacy (step 5).
3. Buy cert + sign (step 4).
4. Gumroad page + upload (step 7).
