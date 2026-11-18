# Home Lab

A small, fast Windows desktop app for keeping an eye on the Raspberry Pis in your home.
It's a single `.exe` written in Rust ([egui](https://github.com/emilk/egui) + [russh](https://github.com/Eugeny/russh)). There's no web server, and nothing needs to be installed on the Pi.

## Features

- **Add a Pi with just its IP address, user and password.** The app logs in once, installs its own SSH key and runs an install script you choose as root. After that it never needs the password again, and the password is never saved.
- **Live status** for each Pi, refreshed every 10 seconds: CPU temperature, load, memory, disk, uptime, model and OS.
- **Diagnostics** for each Pi:
  - Speedtest: LAN speed (PC ↔ Pi over SSH) and the Pi's internet download, upload and ping (via Cloudflare)
  - Network: SSH response time, router ping, internet ping, DNS, Wi-Fi signal, IP address
  - Health: under-voltage and throttling, failed services, pending updates, errors in the system log, time sync, swap, kernel, top processes

## Download

Get the latest version from [Releases](../../releases/latest):

| Platform | File |
|---|---|
| Windows 10/11 | `homelab-windows-x64.exe` |
| Linux (x64) | `homelab-linux-x64.tar.gz` |
| macOS, Apple Silicon | `homelab-macos-apple-silicon.zip` |
| macOS, Intel | `homelab-macos-intel.zip` |

The macOS app isn't signed. The first time you open it, right-click it and choose *Open*.

## Making a release

GitHub Actions builds every platform on each push to `main`. To publish a release, push a tag:

```sh
git tag v0.1.0
git push origin v0.1.0
```

## Build it yourself

You need Rust. On Linux you also need the GUI libraries (`libxkbcommon-dev libgl1-mesa-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev libwayland-dev`).

On Windows without Visual Studio, use the GNU toolchain with [WinLibs MinGW](https://winlibs.com/):

```sh
cargo build --release
# → target/release/homelab.exe
```

`.cargo/config.toml` points the GNU linker at `C:/Users/Public/mingw64`, because MinGW's linker breaks on paths with spaces. Change that path if your MinGW lives somewhere else.

## Where data is stored

Your list of Pis (`pis.json`) and the app's SSH key (`id_ed25519`) are stored in:

- Windows: `%APPDATA%\homelab`
- macOS: `~/Library/Application Support/homelab`
- Linux: `~/.config/homelab`

## Good to know

- The Pi's host key is accepted without checking. That's fine on a trusted home network.
- The speedtest needs `curl` on the Pi. The default install script installs it.

Icons: [Font Awesome Free](https://fontawesome.com) (SIL OFL 1.1 / CC BY 4.0).
