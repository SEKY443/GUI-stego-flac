# stego-flac desktop — frontend

React + TypeScript + Vite + Tailwind UI for the Tauri app whose Rust backend
lives in [`../../crates/audio-modem-gui`](../../crates/audio-modem-gui). This
directory holds only the webview UI; `tauri.conf.json` and the Rust commands
it calls over IPC live in that crate, not here.

## Develop

From the repository root:

```sh
cd apps/desktop && npm install   # once
cd ../../crates/audio-modem-gui
cargo tauri dev                  # requires `cargo install tauri-cli --version "^2"`
```

`cargo tauri dev` starts the Vite dev server itself (via `beforeDevCommand` in
`tauri.conf.json`) and opens the app window pointed at it with hot reload.

## Build

```sh
cd crates/audio-modem-gui
cargo tauri build
```

Produces a native installer under `target/release/bundle/`: `.dmg` on macOS,
`.msi`/`.nsis` on Windows, `.deb`/`.rpm`/`.AppImage` on Linux. Linux builds
need the WebKitGTK runtime; see the root README / CI workflow for the exact
package name.

**Don't run `target/release/stego-flac-desktop` (or `cargo run`) directly on
macOS** and expect a working window. WKWebView needs the app's `.app` bundle
context (`Contents/Info.plist`, bundle identifier) to register the custom
protocol it loads the frontend from; the loose Mach-O binary lacks that and
the window comes up blank white with a `-1004` ("cannot connect to host")
load failure in the system log — it silently falls through to a real network
request for a host that doesn't exist. Always launch the bundled
`target/{debug,release}/bundle/macos/stego-flac.app` (`open .../stego-flac.app`),
which `cargo tauri build`/`cargo tauri build --debug` produce.

## Layout

```
src/
  types.ts             TypeScript mirror of the Rust command DTOs
  api.ts                typed wrappers around Tauri's invoke()/dialog plugin
  components/
    ui.tsx              shared primitives (Field, Button, Banner, ...)
    PlanFields.tsx       tone-plan form, shared by Encode and Plan explorer
    EncodeView.tsx
    DecodeView.tsx
    InfoView.tsx
    PlanExplorerView.tsx
```

No stego-flac logic lives here — every view only renders state and calls a
Rust command; encoding, decoding, and plan resolution all happen in
`audio-modem-core`/`audio-modem-io` on the other side of the IPC boundary.
