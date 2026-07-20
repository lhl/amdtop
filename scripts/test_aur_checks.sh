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

mock_indexed_version=0.2.5-1
curl() {
  printf '{"results":[{"Version":"%s"}]}\n' "$mock_indexed_version"
}
sleep_count=0
sleep() {
  sleep_count=$((sleep_count + 1))
}
AUR_INDEXED_VERSION=''
aur_wait_for_index 0.2.5-1 3 0
[[ "$AUR_INDEXED_VERSION" == 0.2.5-1 ]] || fail 'indexed version was not retained'
[[ "$sleep_count" == 0 ]] || fail 'successful index check slept unexpectedly'

mock_indexed_version=0.2.4-1
AUR_INDEXED_VERSION=''
expect_failure aur_wait_for_index 0.2.5-1 3 0
[[ "$AUR_INDEXED_VERSION" == 0.2.4-1 ]] || fail 'stale indexed version was not retained'
[[ "$sleep_count" == 2 ]] || fail "expected two polling sleeps, got $sleep_count"
expect_failure aur_wait_for_index 0.2.5-1 0 0
expect_failure aur_wait_for_index 0.2.5-1 1 invalid

printf 'AUR release checks passed\n'
