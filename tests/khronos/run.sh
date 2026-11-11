#!/usr/bin/env bash
# Run the Khronos corpus tests locally with the pinned validator (see docs/VALIDATION.md).
# Optional: GLTF_VALIDATOR_SHA256=<sha256 of the .tar.xz> to verify the download.
set -euo pipefail
VERSION=2.0.0-dev.3.10
root="$(cd "$(dirname "$0")/../.." && pwd)"
dir="$root/target/khronos"
bin="$dir/gltf_validator"
if [ -n "${GLTF_VALIDATOR:-}" ]; then
  bin="$GLTF_VALIDATOR"
elif [ ! -x "$bin" ]; then
  mkdir -p "$dir"
  tarball="$dir/validator.tar.xz"
  curl -fsSL -o "$tarball" "https://github.com/KhronosGroup/glTF-Validator/releases/download/$VERSION/gltf_validator-$VERSION-linux64.tar.xz"
  if [ -n "${GLTF_VALIDATOR_SHA256:-}" ]; then
    echo "$GLTF_VALIDATOR_SHA256  $tarball" | sha256sum -c -
  else
    echo "warning: GLTF_VALIDATOR_SHA256 not set; download is pinned by version only" >&2
  fi
  tar -xJf "$tarball" -C "$dir"
  test -x "$bin"
fi
cd "$root"
GLTF_VALIDATOR="$bin" cargo test --test corpus -- --ignored
