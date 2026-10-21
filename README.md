# Radio4000 desktop

Native Radio4000 player, but mostly exploring building native apps with rust. 

Play the audio of one URL through libmpv, which resolves YouTube and friends.

## Install

Linux and macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/oskarrough/r4-desktop-test/main/install.sh | sh
```

Windows:

```powershell
irm https://raw.githubusercontent.com/oskarrough/r4-desktop-test/main/install.ps1 | iex
```

Run the same line again to update.

<details>

<summary>How to develop locally</summary>


## Running from source

```sh
cargo run
```

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
