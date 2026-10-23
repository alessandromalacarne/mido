#!/bin/sh
set -eu

repo="alessandromalacarne/mido"
target="x86_64-unknown-linux-musl"

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

os="$(uname -s)"
arch="$(uname -m)"
if [ "$os" != "Linux" ] || [ "$arch" != "x86_64" ]; then
  fail "no release binary for $os/$arch — install with: cargo install mido-guard"
fi

for tool in curl tar sha256sum install; do
  command -v "$tool" >/dev/null 2>&1 || fail "required tool not found: $tool"
done

latest="$(curl -fsSL -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest")" \
  || fail "could not resolve the latest release"
tag="${latest##*/}"
[ -n "$tag" ] || fail "could not resolve the latest release tag"
version="${tag#v}"
archive="mido-v${version}-${target}.tar.gz"
base="https://github.com/$repo/releases/download/$tag"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

curl -fsSL "$base/$archive" -o "$tmp/$archive" || fail "could not download $archive"
curl -fsSL "$base/SHA256SUMS" -o "$tmp/SHA256SUMS" || fail "could not download SHA256SUMS"

( cd "$tmp" && grep " $archive\$" SHA256SUMS | sha256sum -c - ) >/dev/null \
  || fail "checksum verification failed for $archive"

tar -xzf "$tmp/$archive" -C "$tmp"
dir="${MIDO_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$dir"
install -m 755 "$tmp/mido-v${version}-${target}/mido" "$dir/mido"

printf 'installed mido %s to %s\n' "$version" "$dir/mido"
case ":$PATH:" in
  *":$dir:"*) ;;
  *) printf "warning: %s is not on PATH; add it with:\n  export PATH=\"%s:\$PATH\"\n" "$dir" "$dir" >&2 ;;
esac
