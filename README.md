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

## Build

You need Rust. If you don't have Visual Studio, use the GNU toolchain with [WinLibs MinGW](https://winlibs.com/):

```sh
cargo build --release
# → target/release/homelab.exe
```

`.cargo/config.toml` points the GNU linker at `C:/Users/Public/mingw64`, because MinGW's linker breaks on paths with spaces. Change that path if your MinGW lives somewhere else.

## Where data is stored

`%APPDATA%\homelab`: your list of Pis (`pis.json`) and the app's SSH key (`id_ed25519`).

## Good to know

- The Pi's host key is accepted without checking. That's fine on a trusted home network.
- The speedtest needs `curl` on the Pi. The default install script installs it.

Icons: [Font Awesome Free](https://fontawesome.com) (SIL OFL 1.1 / CC BY 4.0).
