# GUI-stego-flac

A desktop client for [stego-flac](https://github.com/SEKY443/stego-flac),
plus its original CLI, in one Cargo workspace. Built with Tauri, targeting
Windows, macOS, and Linux from a single codebase.

stego-flac hides a file inside an acoustic waveform stored as a standard FLAC
file: the payload is compressed, encrypted (AES-256-GCM, Argon2id-derived
key), erasure-coded (RaptorQ), and modulated as OFDM/QAM or M-FSK into a
mono 16-bit FLAC that any audio player will accept. See
[`docs/cli-reference.md`](docs/cli-reference.md) for the full design writeup —
frame format, threat model, throughput numbers, and limitations — carried over
from upstream.

## Desktop app

Four views, covering the CLI's full flag surface with no reduced feature set:

- **Encode** — file + passphrase, with an Advanced panel for profile,
  compression level, FEC overhead/symbol size, channel count, and radio-mode
  cover audio (quality/mode/attenuation/metadata). A live plan preview updates
  as you change settings, and progress/cancel is reported during the encode.
- **Decode** — reads the carrier's header first (profile, size, whether it's
  encrypted) before asking for a passphrase, matching the CLI's own ordering.
- **Info** — read-only inspection of a carrier's header, tone plan, and
  truncation status; never asks for a passphrase.
- **Plan explorer** — a standalone calculator for every OFDM/FSK tone-plan
  parameter, mirroring `stego-flac plan`.

The Rust backend (`audio-modem-gui`) calls `audio-modem-core`/`audio-modem-io`
directly — it never shells out to the `stego-flac` binary. Theme follows the
OS light/dark setting automatically.

## Download

Prebuilt installers for all three platforms are attached to the
[latest release](https://github.com/SEKY443/GUI-stego-flac/releases/latest):

| Platform | File |
|---|---|
| macOS (Apple Silicon) | `.dmg` |
| Windows (x64) | `.exe` (NSIS) or `.msi` |
| Linux (x64) | `.AppImage`, `.deb`, or `.rpm` |

These are unsigned builds (no Apple/Microsoft developer certificate yet), so
macOS will warn "unidentified developer" on first launch (right-click → Open)
and Windows SmartScreen will warn "unrecognized app" (More info → Run
anyway). The app itself is unaffected either way.

## Platform status

`cargo test --workspace` and `cargo tauri build` both pass in CI on
`macos-latest`, `windows-latest`, and `ubuntu-latest` — see the
[`build` GitHub Actions workflow](.github/workflows/build.yml). macOS was
additionally verified locally by hand (launching the app, exercising the
encode/decode flow, confirming the theme by screenshot); Windows and Linux
have only been verified through that CI run, not by a person clicking through
the app on those OSes. See [`docs/packaging.md`](docs/packaging.md) for the
handful of macOS-specific packaging gotchas found and fixed along the way, in
case either has a Windows/Linux analogue worth watching for.

## Layout

```
crates/
  audio-modem-core/   container-independent DSP and coding (vendored, unmodified)
  audio-modem-io/      FLAC I/O, Vorbis-comment tags, cover-audio loading — shared
                        by the CLI and the GUI so neither duplicates the other
  audio-modem-cli/     the original `stego-flac` command-line binary
  audio-modem-gui/     Tauri (Rust) backend for the desktop app
apps/desktop/          Tauri frontend (React + TypeScript)
test/                  demo carriers used for manual verification
docs/cli-reference.md  the full upstream design/usage writeup
```

`audio-modem-core` is untouched from upstream. `audio-modem-cli`'s FLAC
container I/O, Vorbis-comment tag handling, and cover-audio loading — which
used to live inside the CLI binary — now live in `audio-modem-io` so the
desktop app calls the exact same code rather than a reimplementation or a
wrapped subprocess. The GUI never shells out to the CLI: it links
`audio-modem-core`/`audio-modem-io` directly, which matters for the same
reason the CLI has no `--passphrase` flag — a secret should never cross a
process boundary where it could land in `ps` output or a subprocess's
environment.

## Building the CLI

```sh
cargo build --release -p audio-modem-cli
# target/release/stego-flac
```

## Building the desktop app

See [`apps/desktop/README.md`](apps/desktop/README.md) for development, and
[`docs/packaging.md`](docs/packaging.md) for building installers and what CI
does and doesn't verify.

## Tests

```sh
cargo test --workspace
```

## Licence

Dual-licensed under Apache-2.0 or MIT, matching upstream — see
[`LICENSE-APACHE`](LICENSE-APACHE) and [`LICENSE-MIT`](LICENSE-MIT).
