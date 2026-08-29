# Packaging

## Targets

`crates/audio-modem-gui/tauri.conf.json` sets `bundle.targets` to `"all"`,
which makes `cargo tauri build` produce every installer format valid for the
host OS it runs on:

| OS | formats |
|---|---|
| Windows | `.msi`, NSIS `.exe` |
| macOS | `.dmg`, `.app` |
| Linux | `.deb`, `.rpm`, `.AppImage` |

All of stego-flac's dependencies are pure Rust — no C toolchain is needed
beyond what Tauri itself requires.

## Linux runtime prerequisite

Tauri's Linux webview is WebKitGTK. Building and running the app needs:

```sh
sudo apt-get install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
```

(package names for Debian/Ubuntu; see the Tauri docs for other distros).
This is the one platform-specific runtime dependency across all three OSes —
Windows uses the system's WebView2 (bundled by the installer), macOS uses the
system WKWebView.

## CI

`.github/workflows/build.yml` runs a matrix over
`ubuntu-latest`/`macos-latest`/`windows-latest`, first `cargo test --workspace
--release` on each, then `cargo tauri build` from `crates/audio-modem-gui`,
uploading whatever installers land in `target/release/bundle/` as build
artifacts.

**Only the macOS build has been run and verified locally** in this
environment (`cargo tauri build`/`--debug` both produce a working
`stego-flac.app`; `cargo test --workspace` is green). The Windows and Linux
legs are believed correct by construction — the dependency graph is pure
Rust, the Tauri config lists standard targets for each OS — but have not
actually been executed anywhere; treat their first real CI run as the actual
test, not this write-up.

Two macOS-specific gotchas hit during local verification, in case either
resurfaces:

- **`beforeDevCommand`/`beforeBuildCommand` run relative to the parent of the
  directory holding `tauri.conf.json`** (`crates/`, following the convention
  that a `src-tauri`-equivalent folder is a sibling of the frontend's project
  root), not relative to `tauri.conf.json` itself and not the cargo workspace
  root. That's why they read `../apps/desktop`, not `apps/desktop` or
  `../../apps/desktop` — get this wrong and `npm run build` fails with an
  `ENOENT` on the wrong `package.json` path.
- **DMG creation can fail with a Finder AppleScript timeout
  (`AppleEvent timed out. (-1712)`) if the screen is locked** — `create-dmg`
  drives Finder to lay out the disk-image window, which needs an unlocked,
  interactive session. The `.app` bundle itself is unaffected; only the `.dmg`
  step fails. Re-run `cargo tauri build` unlocked if this happens. CI runners
  keep an active console session, so this is a local-machine-only concern.

## Code signing / notarization

Deliberately not configured. Signing needs the maintainer's own certificates
(Apple Developer ID for notarization, an Authenticode certificate for
Windows, or an APT/RPM repo key for a Linux repo). Without it:

- macOS: Gatekeeper will warn on first launch ("unidentified developer");
  users can still open it via right-click → Open.
- Windows: SmartScreen will show an "unrecognized app" warning.
- Linux: no equivalent warning; unsigned `.deb`/`.rpm`/`.AppImage` install
  normally.

Wiring up signing is a follow-up once the maintainer has the relevant
certificates — `tauri-action`/`cargo tauri build` both support it via
environment variables (`APPLE_CERTIFICATE`, `WINDOWS_CERTIFICATE`, etc.)
without any code changes here.
