#!/bin/sh
# Install or update radio4000 from the latest release.
#   curl -fsSL https://raw.githubusercontent.com/oskarrough/r4-desktop-test/main/install.sh | sh
set -eu

repo="oskarrough/r4-desktop-test"
bin_dir="${RADIO4000_BIN_DIR:-$HOME/.local/bin}"

case "$(uname -s)-$(uname -m)" in
	Linux-x86_64) asset="radio4000-linux-x86_64.tar.gz" ;;
	Darwin-arm64) asset="radio4000-macos-arm64.tar.gz" ;;
	*)
		echo "radio4000: no build for $(uname -s) $(uname -m) yet. Build from source: https://github.com/$repo#running-from-source" >&2
		exit 1
		;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $asset"
curl -fsSL "https://github.com/$repo/releases/download/latest/$asset" -o "$tmp/$asset"
tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$bin_dir"
# Move rather than copy over, so updating works while an old radio4000 is still playing.
mv -f "$tmp/radio4000" "$bin_dir/radio4000"
chmod +x "$bin_dir/radio4000"
echo "Installed $bin_dir/radio4000"

case ":$PATH:" in
	*":$bin_dir:"*) ;;
	*) echo "$bin_dir is not on your PATH. Add this to your shell profile: export PATH=\"$bin_dir:\$PATH\"" ;;
esac

# Check the runtime dependencies; install nothing.
missing_mpv=0
if [ "$(uname -s)" = Linux ]; then
	if ldd "$bin_dir/radio4000" 2>/dev/null | grep -q 'libmpv.*not found'; then missing_mpv=1; fi
else
	brew_prefix="$(brew --prefix 2>/dev/null || echo /opt/homebrew)"
	[ -e "$brew_prefix/lib/libmpv.2.dylib" ] || missing_mpv=1
fi
missing_ytdlp=0
command -v yt-dlp >/dev/null 2>&1 || missing_ytdlp=1

if [ "$missing_mpv$missing_ytdlp" != 00 ]; then
	pkgs=""
	if command -v brew >/dev/null 2>&1 && [ "$(uname -s)" = Darwin ]; then
		[ $missing_mpv = 1 ] && pkgs="$pkgs mpv"
		[ $missing_ytdlp = 1 ] && pkgs="$pkgs yt-dlp"
		cmd="brew install$pkgs"
	elif command -v dnf >/dev/null 2>&1; then
		[ $missing_mpv = 1 ] && pkgs="$pkgs mpv-libs"
		[ $missing_ytdlp = 1 ] && pkgs="$pkgs yt-dlp"
		cmd="sudo dnf install$pkgs"
	elif command -v apt-get >/dev/null 2>&1; then
		[ $missing_mpv = 1 ] && pkgs="$pkgs libmpv2"
		[ $missing_ytdlp = 1 ] && pkgs="$pkgs yt-dlp"
		cmd="sudo apt install$pkgs"
	elif command -v pacman >/dev/null 2>&1; then
		[ $missing_mpv = 1 ] && pkgs="$pkgs mpv"
		[ $missing_ytdlp = 1 ] && pkgs="$pkgs yt-dlp"
		cmd="sudo pacman -S$pkgs"
	else
		[ $missing_mpv = 1 ] && pkgs="$pkgs libmpv"
		[ $missing_ytdlp = 1 ] && pkgs="$pkgs yt-dlp"
		cmd="install$pkgs with your package manager"
	fi
	echo "radio4000 needs$pkgs to play. Run: $cmd"
fi
