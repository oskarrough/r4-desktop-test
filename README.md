# Radio4000 desktop

Native Radio4000 player. Right now just a playback spike: play the audio of one URL through libmpv, which resolves YouTube and friends with yt-dlp.

## Running

```sh
cargo run -- https://www.youtube.com/watch?v=1JLVbkHieaM
```

Prints `playing: <title>` when the track loads and `ended: <reason>` when it stops. Ctrl-C quits.

Needs libmpv (API 2.x) to link against and `yt-dlp` on `PATH`.

### Linux

Install the libmpv dev package and yt-dlp.

- Fedora: `sudo dnf install mpv-devel yt-dlp` (mpv lives in RPM Fusion)
- Debian/Ubuntu: `sudo apt install libmpv-dev yt-dlp`
- Arch: `sudo pacman -S mpv yt-dlp`

### macOS

```sh
brew install mpv yt-dlp
```

On Apple Silicon, tell the linker where Homebrew lives: `export LIBRARY_PATH="$(brew --prefix)/lib"`.

### Windows

1. Download `mpv-dev-x86_64-*.7z` from [shinchiro's mpv builds](https://github.com/shinchiro/mpv-winbuild-cmake/releases) and unpack it to e.g. `C:\mpv-dev`.
2. The MSVC toolchain wants `mpv.lib`. Make it from the bundled `mpv.def` in a Developer PowerShell: `lib /def:mpv.def /machine:x64 /out:mpv.lib`.
3. Point the linker at it: `$env:LIB += ";C:\mpv-dev"`.
4. At runtime the exe needs `libmpv-2.dll`. Copy it next to `target\debug\radio4000.exe` or add `C:\mpv-dev` to `PATH`.
5. Install yt-dlp (`winget install yt-dlp`) so it is on `PATH`.
