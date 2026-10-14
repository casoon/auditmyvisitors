---
title: Installation
description: Install the release binary on macOS or Linux, download it on Windows, or build it from source.
order: 1
---

## macOS and Linux

```sh
curl -fsSL https://raw.githubusercontent.com/casoon/auditmyvisitors/main/install.sh | bash
```

The script detects the platform, asks the GitHub API for the latest release and downloads the
matching binary to `~/.local/bin`. Set `INSTALL_DIR` to install somewhere else:

```sh
curl -fsSL https://raw.githubusercontent.com/casoon/auditmyvisitors/main/install.sh | INSTALL_DIR="$HOME/bin" bash
```

If the directory is not on your `PATH`, the script prints the `export PATH=…` line to add to
`~/.zshrc` or `~/.bashrc`.

Supported by the script:

| Platform | Release artefact |
| --- | --- |
| macOS, Apple Silicon (`arm64`) | `auditmyvisitors-macos-arm64` |
| Linux `x86_64` (static, musl) | `auditmyvisitors-linux-x86_64` |

Intel Macs are no longer supported by the release builds. On an Apple Silicon Mac the script
refuses to run in a Rosetta shell (it then sees `x86_64`); open a native terminal and run it
again. On an Intel Mac, build from source.

## Windows

Download `auditmyvisitors-windows-x86_64.exe` from the
[latest release](https://github.com/casoon/auditmyvisitors/releases/latest) and put it in a
directory on your `PATH`.

## Build from source

You need Rust 1.88 or newer ([rustup.rs](https://rustup.rs)) and OAuth credentials of a Google
Cloud project. The client ID and secret are embedded at compile time; a build without them
works, but cannot log in.

```sh
git clone https://github.com/casoon/auditmyvisitors
cd auditmyvisitors

# Credentials go into .env.local, which is git-ignored.
cat > .env.local <<'ENV'
export GOOGLE_CLIENT_ID=...
export GOOGLE_CLIENT_SECRET=...
ENV

make release
./target/release/auditmyvisitors --help
```

`make release` sources `.env.local` and runs `cargo build --release`; `make build` does the
same for a debug build.

## Check the installation

```sh
auditmyvisitors --version
```

```text
auditmyvisitors 0.3.1
```

Next: the [Quickstart](../quickstart/).
