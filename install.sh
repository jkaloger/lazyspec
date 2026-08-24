#!/bin/sh
# Install the latest lazyspec release binary.
#
#   curl -fsSL https://raw.githubusercontent.com/jkaloger/lazyspec/main/install.sh | sh
#
# Environment:
#   LAZYSPEC_VERSION      release tag to install (e.g. v0.12.0); default: latest
#   LAZYSPEC_INSTALL_DIR  install directory; default: ~/.local/bin
set -eu

repo="jkaloger/lazyspec"
install_dir="${LAZYSPEC_INSTALL_DIR:-$HOME/.local/bin}"

os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Darwin)
    case "$arch" in
      arm64) target="aarch64-apple-darwin" ;;
      x86_64) target="x86_64-apple-darwin" ;;
      *) echo "error: unsupported macOS architecture: $arch" >&2; exit 1 ;;
    esac
    ;;
  Linux)
    case "$arch" in
      aarch64 | arm64) target="aarch64-unknown-linux-musl" ;;
      x86_64) target="x86_64-unknown-linux-musl" ;;
      *) echo "error: unsupported Linux architecture: $arch" >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "error: unsupported OS: $os (prebuilt binaries cover macOS and Linux; try 'cargo install lazyspec')" >&2
    exit 1
    ;;
esac

tag="${LAZYSPEC_VERSION:-}"
if [ -z "$tag" ]; then
  tag="$(curl -fsSL "https://api.github.com/repos/$repo/releases/latest" \
    | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)"
fi
if [ -z "$tag" ]; then
  echo "error: could not determine latest release tag" >&2
  exit 1
fi

archive="lazyspec-$tag-$target.tar.gz"
url="https://github.com/$repo/releases/download/$tag/$archive"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "downloading $url"
curl -fsSL "$url" -o "$tmp/$archive"
curl -fsSL "$url.sha256" -o "$tmp/$archive.sha256"

# macOS ships shasum, minimal Linux images ship sha256sum; accept either.
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$tmp" && sha256sum -c "$archive.sha256" >/dev/null)
elif command -v shasum >/dev/null 2>&1; then
  (cd "$tmp" && shasum -a 256 -c "$archive.sha256" >/dev/null)
else
  echo "error: need sha256sum or shasum to verify the download" >&2
  exit 1
fi

tar -xzf "$tmp/$archive" -C "$tmp" lazyspec
mkdir -p "$install_dir"
install -m 755 "$tmp/lazyspec" "$install_dir/lazyspec"

echo "installed lazyspec $tag to $install_dir/lazyspec"
case ":$PATH:" in
  *":$install_dir:"*) ;;
  *) echo "note: $install_dir is not on your PATH" >&2 ;;
esac
