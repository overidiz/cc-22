# Code Signing — CC-22 (Windows)

Unsigned binaries make Windows show a **"Windows protected your PC / unknown
publisher"** SmartScreen warning. For a paid product you want this gone. This is
the practical path.

## 1. Choose a certificate

| Type | SmartScreen behaviour | Cost (approx) | Notes |
|------|----------------------|---------------|-------|
| **OV** (Organization Validation) | Reputation builds up over downloads; early users may still see a warning | ~US$70–200/yr | Cheapest; fine if you're patient |
| **EV** (Extended Validation) | Trusted **immediately**, no warm-up | ~US$250–400/yr | Best UX; usually requires a hardware token / cloud HSM |

Vendors: **Sectigo, SSL.com, DigiCert, GlobalSign**. For a solo dev, SSL.com and
Sectigo are usually the cheapest. You'll need to prove identity (OV: business or
sole-trader docs; EV: stricter).

> Certificates now generally require the private key on an **HSM / hardware token
> or a cloud signing service** (the CA will explain their flow). You sign by
> pointing `signtool` at that token/service.

## 2. Install `signtool`

`signtool.exe` ships with the **Windows SDK** (or Visual Studio "Desktop
development with C++"). Typical location:
```
C:\Program Files (x86)\Windows Kits\10\bin\<version>\x64\signtool.exe
```

## 3. Sign the payload, then the installer

Order matters: sign the plugin/standalone binaries **first**, then rebuild the
installer so it packages the signed files, then sign the installer.

```bat
:: 1) sign the three binaries (adjust cert selection to your CA's instructions)
signtool sign /fd SHA256 /tr http://timestamp.sectigo.com /td SHA256 ^
  ".\target\bundled\CC-22.vst3\Contents\x86_64-win\CC-22.vst3" ^
  ".\target\bundled\CC-22.clap" ^
  ".\target\bundled\CC-22.exe"

:: 2) rebuild the installer so it embeds the signed payload
"C:\Users\Rafa\tools\InnoSetup\ISCC.exe" installer\cc22.iss

:: 3) sign the installer itself
signtool sign /fd SHA256 /tr http://timestamp.sectigo.com /td SHA256 ^
  ".\installer\output\CC-22-1.0.0-Setup.exe"
```

`/tr` + `/td` add a **timestamp** so signatures stay valid after the cert expires.
If your cert is in the Windows cert store you can add `/a` to auto-select it; with
a token/cloud HSM, follow the CA's `signtool` flags (often `/csp` + `/kc` or a
dlib).

## 4. Verify

```bat
signtool verify /pa ".\installer\output\CC-22-1.0.0-Setup.exe"
signtool verify /pa ".\target\bundled\CC-22.vst3\Contents\x86_64-win\CC-22.vst3"
```

Then double-check by downloading your own installer in a browser and confirming
no "unknown publisher" warning appears.

## 5. Automate it in Inno Setup (optional)

Once `signtool` works, you can have the installer sign itself:
```
[Setup]
SignTool=mysigntool $f
SignedUninstaller=yes
```
…with `mysigntool` defined in the Inno Setup IDE (Tools → Configure Sign Tools).

## Future: macOS
macOS uses a different system — an **Apple Developer ID** certificate (US$99/yr
Apple Developer Program) plus **notarization** (`codesign` + `notarytool`). Only
relevant once you ship a Mac build.
