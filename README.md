# Home Lab

A small, fast desktop app (Windows, Linux, macOS) for keeping an eye on the Raspberry Pis in your home.
It's a single program written in Rust ([egui](https://github.com/emilk/egui) + [russh](https://github.com/Eugeny/russh)). There's no web server, and nothing needs to be installed on the Pi.

## Features

- **Add a Pi with just its IP address, user and password.** The app logs in once, installs its own SSH key and runs an install script you choose as root. After that it never needs the password again, and the password is never saved.
- **Live status** for each Pi, refreshed every 10 seconds over one kept-open SSH connection: CPU temperature (with a 10-minute graph), load, memory, disk, uptime, model and OS. The last known values show instantly when the app starts.
- **Alerts** when a Pi goes offline, comes back online or runs hot.
- **Diagnostics** for each Pi:
  - Speedtest: LAN speed (PC ↔ Pi over SSH) and the Pi's internet download, upload and ping (via Cloudflare)
  - Network: SSH response time, router ping, internet ping, DNS, Wi-Fi signal, IP address
  - Health: under-voltage and throttling, failed services, pending updates, errors in the system log, time sync, swap, kernel, top processes
- **Actions:** update packages, reboot or shut down a Pi (with confirmation; an optional sudo password is never saved).
- **One-click SSH terminal** to a Pi, logged in with the app's key.
- **Edit** a Pi's name or connection, **retry** a failed setup, and **drag** cards by their header to reorder them.
- **Secure by default:** each Pi's SSH host key is remembered and the app refuses to connect if it changes.

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

Your list of Pis (`pis.json`), the last known stats (`cache.json`) and the app's SSH key (`id_ed25519`) are stored in:

- Windows: `%APPDATA%\homelab`
- macOS: `~/Library/Application Support/homelab`
- Linux: `~/.config/homelab`

## Alerts

The app shows a banner (and flashes its taskbar icon) when a Pi goes offline, comes back online, or its CPU reaches 75 °C. To change the temperature limit, set the `HOMELAB_HOT_C` environment variable, e.g. `HOMELAB_HOT_C=70`.

## Good to know

- The Pi's host key is trusted the first time the app connects ("trust on first use") and checked on every connection after that. If you reinstall a Pi, use *Edit → Run setup again…*.
- The speedtest needs `curl` on the Pi. The default install script installs it.

Icons: [Font Awesome Free](https://fontawesome.com) (SIL OFL 1.1 / CC BY 4.0).
