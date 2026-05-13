# HealthyReminders

A lightweight healthy reminders app to notify you to drink water, rest your eyes, and stand up written in Rust for Windows.

## Requirements

- Windows 10/11.
- Rust stable with the `x86_64-pc-windows-msvc` target.
- Visual Studio Build Tools with MSVC toolchain and Windows SDK.

## Build

```powershell
rustup target add x86_64-pc-windows-msvc
cargo build --release --target x86_64-pc-windows-msvc
```

The binary file is located at:

```text
target\x86_64-pc-windows-msvc\release\HealthyReminders.exe
```