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

mock_now=0
aur_now_seconds() {
  printf '%s\n' "$mock_now"
}

mock_indexed_version=0.2.5-1
mock_fail_until_sleep=0
mock_request_timeout_limit=3
curl_log="$tmp_dir/curl-log"
curl() {
  local connect_timeout=''
  local max_time=''

  while (( $# > 0 )); do
    case "$1" in
      --connect-timeout)
        connect_timeout=$2
        shift 2
        ;;
      --max-time)
        max_time=$2
        shift 2
        ;;
      *)
        shift
        ;;
    esac
  done

  [[ "$connect_timeout" =~ ^[1-9][0-9]*$ ]] || return 97
  [[ "$max_time" =~ ^[1-9][0-9]*$ ]] || return 97
  ((connect_timeout <= mock_request_timeout_limit)) || return 97
  ((max_time <= mock_request_timeout_limit)) || return 97
  printf '%s %s\n' "$connect_timeout" "$max_time" >>"$curl_log"

  ((sleep_count >= mock_fail_until_sleep)) || return 22
  printf '{"results":[{"Version":"%s"}]}\n' "$mock_indexed_version"
}
sleep_count=0
sleep() {
  sleep_count=$((sleep_count + 1))
  mock_now=$((mock_now + $1))
}

AUR_INDEXED_VERSION=''
aur_wait_for_index 0.2.5-1 10 2 3
[[ "$AUR_INDEXED_VERSION" == 0.2.5-1 ]] || fail 'indexed version was not retained'
[[ "$sleep_count" == 0 ]] || fail 'successful index check slept unexpectedly'

: >"$curl_log"
mock_now=0
sleep_count=0
mock_indexed_version=0.2.4-1
mock_request_timeout_limit=2
AUR_INDEXED_VERSION=''
expect_failure aur_wait_for_index 0.2.5-1 3 1 2
[[ "$AUR_INDEXED_VERSION" == 0.2.4-1 ]] || fail 'stale indexed version was not retained'
[[ "$sleep_count" == 2 ]] || fail "expected two polling sleeps, got $sleep_count"
[[ "$(wc -l <"$curl_log")" == 3 ]] || fail 'deadline did not bound polling attempts'

: >"$curl_log"
mock_now=0
sleep_count=0
mock_indexed_version=0.2.5-1
mock_fail_until_sleep=1
mock_request_timeout_limit=2
AUR_INDEXED_VERSION=''
aur_wait_for_index 0.2.5-1 10 1 2
[[ "$AUR_INDEXED_VERSION" == 0.2.5-1 ]] || fail 'polling did not recover after a curl failure'
[[ "$sleep_count" == 1 ]] || fail 'transient failure did not retry once'

expect_failure aur_wait_for_index 0.2.5-1 0 1 1
expect_failure aur_wait_for_index 0.2.5-1 1 0 1
expect_failure aur_wait_for_index 0.2.5-1 1 1 invalid

printf 'AUR release checks passed\n'
