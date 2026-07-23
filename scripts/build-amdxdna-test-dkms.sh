#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
package_dir="$script_dir/amdxdna-test-dkms"

usage() {
  cat <<'EOF'
Usage: scripts/build-amdxdna-test-dkms.sh [--verify-only | --print-package-dir]

Build the pinned Arch amdxdna DKMS test package used for amdtop NPU telemetry.
The script does not install the resulting package or change the running driver.

Options:
  --verify-only        Download and verify pinned sources without packaging
  --print-package-dir  Print the package recipe directory and exit
  -h, --help           Show this help
EOF
}

mode=build
while (($#)); do
  case "$1" in
    --verify-only) mode=verify ;;
    --print-package-dir) mode=print-dir ;;
    -h|--help) usage; exit 0 ;;
    *) printf 'unknown option: %s\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

if [[ "$mode" == print-dir ]]; then
  printf '%s\n' "$package_dir"
  exit 0
fi

command -v makepkg >/dev/null || {
  printf 'makepkg is required; run this script on Arch Linux or a derivative\n' >&2
  exit 1
}

cd "$package_dir"
if [[ "$mode" == verify ]]; then
  exec makepkg --verifysource
fi

makepkg --cleanbuild --force
package=$(find "$package_dir" -maxdepth 1 -type f \
  -name 'amdxdna-amdtop-dkms-*.pkg.tar.zst' -printf '%T@ %p\n' |
  sort -nr | awk 'NR == 1 { sub(/^[^ ]+ /, ""); print; exit }')
[[ -n "$package" ]] || {
  printf 'package build completed but no package archive was found\n' >&2
  exit 1
}

printf '\nBuilt: %s\n' "$package"
printf 'Inspect: pacman -Qip %q\n' "$package"
printf 'Install: sudo pacman -U %q\n' "$package"
