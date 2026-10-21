#!/bin/sh
# Install or update radio4000 from the latest release.
#   curl -fsSL https://raw.githubusercontent.com/oskarrough/r4-desktop-test/main/install.sh | sh
set -eu

repo="oskarrough/r4-desktop-test"
bin_dir="${RADIO4000_BIN_DIR:-$HOME/.local/bin}"

fail() {
	echo "radio4000: $1" >&2
	exit 1
}

os="$(uname -s)"
arch="$(uname -m)"
# Under Rosetta, uname reports x86_64 on an Apple Silicon Mac.
if [ "$os-$arch" = Darwin-x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null)" = 1 ]; then
	arch=arm64
fi
case "$os-$arch" in
	Linux-x86_64) asset="radio4000-linux-x86_64.tar.gz" ;;
	Darwin-arm64) asset="radio4000-macos-arm64.tar.gz" ;;
	*) fail "no build for $os $arch yet. Build from source: https://github.com/$repo#running-from-source" ;;
esac

if command -v sha256sum >/dev/null 2>&1; then
	checksum() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
	checksum() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
	fail "sha256sum or shasum is required"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $asset"
url="https://github.com/$repo/releases/download/latest"
curl -fsSL "$url/SHA256SUMS" -o "$tmp/SHA256SUMS" || fail "could not download SHA256SUMS"
curl -fsSL "$url/$asset" -o "$tmp/$asset" || fail "could not download $asset"
expected="$(grep " \*\{0,1\}$asset\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)"
[ -n "$expected" ] || fail "$asset is not listed in SHA256SUMS"
[ "$(checksum "$tmp/$asset")" = "$expected" ] || fail "checksum mismatch for $asset; nothing installed"
mkdir "$tmp/new"
tar -xzf "$tmp/$asset" -C "$tmp/new"
new="$tmp/new/radio4000"

# Check the runtime dependencies; install nothing.
missing_mpv=0
if [ "$os" = Linux ]; then
	if ldd "$new" 2>/dev/null | grep -q 'libmpv.*not found'; then missing_mpv=1; fi
else
	brew_prefix="$(brew --prefix 2>/dev/null || echo /opt/homebrew)"
	[ -e "$brew_prefix/lib/libmpv.2.dylib" ] || missing_mpv=1
fi
missing_ytdlp=0
command -v yt-dlp >/dev/null 2>&1 || missing_ytdlp=1

# Make sure the new binary runs before it replaces the old one. Without libmpv it can't
# start at all, so then it's installed untried and the missing-deps line below says why.
if [ $missing_mpv = 0 ]; then
	if ! new_version="$("$new" --version 2>&1)"; then
		[ -z "$new_version" ] || echo "$new_version" >&2
		fail "the downloaded radio4000 doesn't run; kept the old install"
	fi
	new_version="${new_version#radio4000 }"
fi
# Builds from before --version existed print something else; treat them as unknown.
old_version=""
if [ -x "$bin_dir/radio4000" ]; then
	old_version="$("$bin_dir/radio4000" --version 2>/dev/null || true)"
	case "$old_version" in
		"radio4000 "*) old_version="${old_version#radio4000 }" ;;
		*) old_version="(older build)" ;;
	esac
fi

mkdir -p "$bin_dir"
# Move rather than copy over, so updating works while an old radio4000 is still playing.
mv -f "$new" "$bin_dir/radio4000"
chmod +x "$bin_dir/radio4000"
if [ $missing_mpv = 1 ]; then
	echo "Installed $bin_dir/radio4000"
elif [ "$old_version" = "$new_version" ]; then
	echo "Reinstalled radio4000 $new_version in $bin_dir"
elif [ -n "$old_version" ]; then
	echo "Updated radio4000 $old_version → $new_version in $bin_dir"
else
	echo "Installed radio4000 $new_version in $bin_dir"
fi

case ":$PATH:" in
	*":$bin_dir:"*) ;;
	*)
		echo "$bin_dir is not on your PATH. Add it with:"
		case "${SHELL:-}" in
			*/fish) echo "  fish_add_path $bin_dir" ;;
			*/zsh) echo "  echo 'export PATH=\"$bin_dir:\$PATH\"' >> ~/.zshrc && source ~/.zshrc" ;;
			*/bash) echo "  echo 'export PATH=\"$bin_dir:\$PATH\"' >> ~/.bashrc && source ~/.bashrc" ;;
			*) echo "  export PATH=\"$bin_dir:\$PATH\"   (in your shell's startup file)" ;;
		esac
		;;
esac

if [ "$missing_mpv$missing_ytdlp" != 00 ]; then
	pkgs=""
	if command -v brew >/dev/null 2>&1 && [ "$os" = Darwin ]; then
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
