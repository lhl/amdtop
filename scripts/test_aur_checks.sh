#!/usr/bin/env bash
set -euo pipefail

readonly repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=aur_checks.sh
source "$repo_root/scripts/aur_checks.sh"

fail() {
  printf 'test failure: %s\n' "$*" >&2
  exit 1
}

expect_failure() {
  "$@" >/dev/null 2>&1 && fail "command unexpectedly passed: $*"
  return 0
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

version=0.2.5
listing="$tmp_dir/archive-listing"
printf '%s\n' \
  "amdtop-$version/Cargo.lock" \
  "amdtop-$version/LICENSE" \
  "amdtop-$version/NOTICE" \
  "amdtop-$version/THIRD_PARTY.md" >"$listing"
aur_check_archive_listing "$listing" "$version"
for missing in Cargo.lock LICENSE NOTICE THIRD_PARTY.md; do
  grep -Fv "amdtop-$version/$missing" "$listing" >"$tmp_dir/missing"
  expect_failure aur_check_archive_listing "$tmp_dir/missing" "$version"
done

srcinfo="$tmp_dir/.SRCINFO"
printf '\tlicense = Apache-2.0\n' >"$srcinfo"
aur_check_srcinfo "$srcinfo"
printf '\tlicense = MIT\n' >"$srcinfo"
expect_failure aur_check_srcinfo "$srcinfo"

package_root="$tmp_dir/package-root"
license_dir="$package_root/usr/share/licenses/amdtop"
mkdir -p "$license_dir"
touch "$license_dir/LICENSE" "$license_dir/NOTICE" "$license_dir/THIRD_PARTY.md"
aur_check_package_notices "$package_root"
for missing in LICENSE NOTICE THIRD_PARTY.md; do
  mv "$license_dir/$missing" "$tmp_dir/$missing"
  expect_failure aur_check_package_notices "$package_root"
  mv "$tmp_dir/$missing" "$license_dir/$missing"
done

printf 'AUR release checks passed\n'
