+++
title = "Install"
description = "Download a release for your system, or build from source."
weight = 1
aliases = ["guide/install/"]
+++

Releases are published on the
[GitHub releases page](https://github.com/NeoTech/Camcraft/releases). Each
release carries builds for Windows x64, Linux x86_64, macOS (universal, Apple
Silicon and Intel), FreeBSD amd64 and Arch Linux.

## Windows

- **Portable zip:** unzip it anywhere and run `camcraft-ui.exe`.
- **Installer** (`camcraft-<version>-win-x64-setup.exe`): installs for the
  current user, without an administrator prompt.

## Linux

- **DEB or RPM:** install the package and start **CamCraft** from the
  application menu, or run `camcraft`.
- **TAR.GZ:** extract it and run `./camcraft-ui` from the extracted folder.
- **AppImage:** `chmod +x` the file and run it; nothing is installed.
- **Snap:** `sudo snap install --dangerous ./camcraft-*.snap`, then run
  `camcraft`.
- **Flatpak:** `flatpak install --user ./camcraft-*.flatpak`, then
  `flatpak run io.github.NeoTech.Camcraft`. The Freedesktop runtime is
  downloaded separately the first time.
- **Arch Linux:** install the `pkg.tar.zst` from the release, or build the
  recipe in the repository's `aur/` folder with `makepkg -si`.

## macOS

Open the DMG and drag `CamCraft.app` to Applications, or extract the ZIP.
The app is ad-hoc signed and not notarized, so the first launch may need
Control-click, then **Open**.

The app runs from `CamCraft.app/Contents/MacOS/camcraft-ui`. The
command-line tool (`camcraft-cli`) and the post processor (`post`) sit in
the same `MacOS` folder. The release files are `camcraft-<version>-macos-universal.dmg`
(the disk image) and `camcraft-<version>-macos-universal.zip` (the portable
archive, which holds `CamCraft.app`).

## FreeBSD

Extract the `freebsd-amd64.tar.gz` archive and run `camcraft-ui` from it.

## What is in the archives

The portable archives carry the desktop app (`camcraft-ui`), the
command-line tool (`camcraft-cli`), the standalone post processor (`post`),
and the assets the app needs: post processors, machine definitions, icons,
fonts and themes.

## Build from source

You need Rust 1.90 or newer.

```sh
cargo run   --release -p camcraft-ui                       # build and start the app
cargo build --release -p camcraft-app --bin camcraft-cli   # headless command line
cargo build --release -p camcraft-post --bin post          # standalone post processor
```

The command line is described, with runnable examples, in
[Command line](@/guide/automate/cli.md).
