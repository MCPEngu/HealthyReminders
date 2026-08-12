# HealthyReminders

A lightweight reminders app for hydration, eye rest, and movement breaks on Windows/Linux.

## Requirements

- Rust stable.
- Windows builds: `x86_64-pc-windows-msvc`, Visual Studio Build Tools, and Windows SDK.
- Linux builds: `x86_64-unknown-linux-gnu` and a desktop notification provider such as `libnotify`.

## Build

Windows:

```powershell
rustup target add x86_64-pc-windows-msvc
cargo build --release --target x86_64-pc-windows-msvc
```

Linux:

```bash
rustup target add x86_64-unknown-linux-gnu
cargo build --release --target x86_64-unknown-linux-gnu
```

The binaries are located at:

```text
Windows: target\x86_64-pc-windows-msvc\release\HealthyReminders.exe

Linux: target/x86_64-unknown-linux-gnu/release/HealthyReminders
```
