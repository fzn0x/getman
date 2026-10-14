#!/bin/sh
# Installs the latest Getman release for this machine, after checking its
# SHA-256 checksum against the release's SHA256SUMS.
#
#   curl -fsSL https://fzn0x.github.io/getman/install.sh | sh
#
# GETMAN_VERSION=v0.1.0 installs a given release instead of the latest.
# GETMAN_INSTALL_DIR chooses the directory; the default is ~/.local/bin.

set -eu

repo="fzn0x/getman"
version="${GETMAN_VERSION:-latest}"
dir="${GETMAN_INSTALL_DIR:-$HOME/.local/bin}"
source_docs="https://github.com/$repo#building-from-source"

say() { printf '%s\n' "$*"; }
fail() {
	printf 'getman install: %s\n' "$*" >&2
	exit 1
}

if command -v curl >/dev/null 2>&1; then
	fetch() { curl -fsSL --proto '=https' --tlsv1.2 -o "$2" "$1"; }
	latest() { curl -fsSIL --proto '=https' --tlsv1.2 -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest"; }
elif command -v wget >/dev/null 2>&1; then
	fetch() { wget -q --https-only -O "$2" "$1"; }
	latest() { wget -q --https-only -S --spider "https://github.com/$repo/releases/latest" 2>&1 | sed -n 's/^ *[Ll]ocation: *//p' | tail -n 1 | tr -d '\r'; }
else
	fail "install curl or wget first"
fi

case "$(uname -s)" in
Linux) os="unknown-linux-gnu" ;;
Darwin) os="apple-darwin" ;;
*) fail "there is no release for $(uname -s); on Windows use install.ps1, or build from source: $source_docs" ;;
esac

case "$(uname -m)" in
x86_64 | amd64) arch="x86_64" ;;
arm64 | aarch64) arch="aarch64" ;;
*) fail "there is no release for $(uname -m); build from source: $source_docs" ;;
esac

# A shell running under Rosetta on an Apple silicon Mac still gets the
# native build.
if [ "$os" = "apple-darwin" ] && [ "$arch" = "x86_64" ] &&
	[ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
	arch="aarch64"
fi
target="$arch-$os"

if [ "$version" = "latest" ]; then
	url="$(latest)" || fail "could not reach GitHub"
	version="${url##*/}"
	case "$version" in
	v*) ;;
	*) fail "could not find the latest release at https://github.com/$repo/releases" ;;
	esac
fi

name="getman-$version-$target"
base="https://github.com/$repo/releases/download/$version"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

say "Downloading Getman $version for $target"
fetch "$base/$name.tar.gz" "$tmp/$name.tar.gz" || fail "could not download $base/$name.tar.gz"
fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || fail "could not download $base/SHA256SUMS"

expected="$(grep " \*\{0,1\}$name.tar.gz\$" "$tmp/SHA256SUMS" | cut -d ' ' -f 1)"
[ -n "$expected" ] || fail "$name.tar.gz is not listed in SHA256SUMS"
if command -v sha256sum >/dev/null 2>&1; then
	actual="$(sha256sum "$tmp/$name.tar.gz" | cut -d ' ' -f 1)"
elif command -v shasum >/dev/null 2>&1; then
	actual="$(shasum -a 256 "$tmp/$name.tar.gz" | cut -d ' ' -f 1)"
else
	fail "sha256sum or shasum is needed to check the download"
fi
[ "$expected" = "$actual" ] || fail "the checksum does not match, so nothing was installed; the download may be damaged or tampered with"

tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
mkdir -p "$dir"
# Replace the binary in one step, so a running getman is not disturbed.
cp "$tmp/$name/getman" "$dir/.getman.new"
chmod 755 "$dir/.getman.new"
mv -f "$dir/.getman.new" "$dir/getman"

say "Installed $dir/getman"
case ":$PATH:" in
*":$dir:"*) say "Run: getman --version" ;;
*)
	say ""
	say "$dir is not on your PATH yet. Add this line to your shell's startup file,"
	say "such as ~/.profile, ~/.bashrc or ~/.zshrc, then open a new terminal:"
	say ""
	say "  export PATH=\"$dir:\$PATH\""
	;;
esac
