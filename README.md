> THIS IS A TEST. Learning how to make and release proper desktop apps for linux, macOS and Windows. 

# Radio4000 desktop

This is a native Radio4000 player for Linux, macOS and Windows.  

Pick a channel, play its tracks. Audio goes through libmpv, which resolves YouTube and friends.

## Install

Linux and macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/radio4000/r4-desktop-test/main/install.sh | sh
```

Windows:

```powershell
irm https://raw.githubusercontent.com/radio4000/r4-desktop-test/main/install.ps1 | iex
```

Radio4000 then opens from your app launcher, Applications on a Mac, or the Start Menu on Windows. Run the same line again to update.

Linux downloads include everything needed to play on Ubuntu 24.04 or newer, and similar desktop systems. You can also download the [AppImage](https://github.com/radio4000/r4-desktop-test/releases/download/latest/radio4000-x86_64.AppImage) directly.

---

<details>
<summary>Keep YouTube playback working</summary>

On first launch Radio4000 downloads yt-dlp, about 40MB, so the first track takes a moment. After that yt-dlp checks for nightly fixes in the background, at most once a day. Reopen Radio4000 to use an update. If the download fails, it keeps the working copy.

Running from source? Install [uv](https://docs.astral.sh/uv/getting-started/installation/), then run:

```sh
uv tool install --upgrade --prerelease allow 'yt-dlp[default]'
uv tool update-shell
```

Restart your terminal afterwards. Run the first command again whenever you want to update.

</details>

<details>

<summary>How to develop locally</summary>


## Running from source

```sh
cargo run
```

Opens the player. With a URL, `cargo run -- https://www.youtube.com/watch?v=1JLVbkHieaM`, it plays just that track without a window: prints `playing: <title>` when the track loads and `ended: <reason>` when it stops. Ctrl-C quits.

Needs libmpv to link against and `yt-dlp` on `PATH`.

### Linux

Install the libmpv dev package and yt-dlp.

- Fedora: `sudo dnf install mpv-devel yt-dlp`
- Debian/Ubuntu: `sudo apt install libmpv-dev yt-dlp`
- Arch: `sudo pacman -S mpv yt-dlp`

### macOS

```sh
brew install mpv yt-dlp
```

### Windows

1. Download `mpv-dev-x86_64-*.7z` from [shinchiro's mpv builds](https://github.com/shinchiro/mpv-winbuild-cmake/releases) and unpack it to e.g. `C:\mpv-dev`.
2. The MSVC toolchain wants `mpv.lib`. Newer builds ship no `mpv.def`, so in a Developer PowerShell write one from the DLL's exports, then build the lib:
   ```powershell
   @('LIBRARY libmpv-2.dll', 'EXPORTS') + (dumpbin /exports libmpv-2.dll | Select-String '^\s+\d+\s+[0-9A-F]+\s+[0-9A-F]{8}\s+(\S+)' | % { $_.Matches[0].Groups[1].Value }) | Set-Content mpv.def
   lib /def:mpv.def /machine:x64 /out:mpv.lib
   ```
3. Point the linker at it: `$env:LIB += ";C:\mpv-dev"`.
4. At runtime the exe needs `libmpv-2.dll`. Copy it next to `target\debug\radio4000.exe` or add `C:\mpv-dev` to `PATH`.
5. Install yt-dlp (`winget install yt-dlp`) so it is on `PATH`.


</details>
