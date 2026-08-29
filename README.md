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

## Platform status

| Platform | Status |
|---|---|
| macOS (Apple Silicon) | **Built and verified locally** — `.app` and `.dmg` both launch and run correctly (light/dark theme confirmed by screenshot, encode/decode flow exercised) |
| Windows | Configured (Tauri `nsis`/`msi` targets, CI job) but **not yet built or run anywhere** — no Windows machine was available to verify it |
| Linux | Configured (Tauri `deb`/`rpm`/`AppImage` targets, CI job) but **not yet built or run anywhere** — no Linux machine was available to verify it |

The Windows and Linux legs are believed correct by construction — every
dependency in the workspace is pure Rust (`#![forbid(unsafe_code)]` in the
core crate, no C toolchain requirements beyond what Tauri itself needs on
each OS), and `tauri.conf.json` lists the standard bundle targets for each —
but "should work" isn't the same claim as "has run." The
[`build` GitHub Actions workflow](.github/workflows/build.yml) will build and
test all three on push; treat its first green run on each OS as the actual
verification, not this paragraph. See [`docs/packaging.md`](docs/packaging.md)
for the macOS-specific gotchas already found and fixed while getting the
local build working, in case either has a Windows/Linux analogue worth
watching for.

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
