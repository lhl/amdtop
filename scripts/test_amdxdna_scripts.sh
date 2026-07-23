#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
build_script="$script_dir/build-amdxdna-test-dkms.sh"
test_script="$script_dir/test-amdxdna-npu.sh"
package_dir="$script_dir/amdxdna-test-dkms"

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

[[ -x "$build_script" ]] || fail "missing executable build script"
[[ -x "$test_script" ]] || fail "missing executable NPU test script"
[[ -f "$package_dir/PKGBUILD" ]] || fail "missing DKMS PKGBUILD"
[[ -f "$package_dir/dkms.conf" ]] || fail "missing DKMS configuration"

actual_package_dir=$($build_script --print-package-dir)
[[ "$actual_package_dir" == "$package_dir" ]] || fail "unexpected package directory: $actual_package_dir"

help=$($test_script --help)
[[ "$help" == *'--runs <count>'* ]] || fail "test help omits --runs"
[[ "$help" == *'--print-workload'* ]] || fail "test help omits --print-workload"

workload=$($test_script --print-workload --runs 3)
[[ "$workload" == 'for _ in 1 2 3; do xrt-smi validate --run gemm --batch || break; done' ]] ||
  fail "unexpected workload command: $workload"

if $test_script --print-workload --runs 0 >/dev/null 2>&1; then
  fail "zero validation runs should be rejected"
fi

srcinfo=$(cd "$package_dir" && makepkg --printsrcinfo)
[[ "$srcinfo" == *$'pkgbase = amdxdna-amdtop-dkms'* ]] || fail "unexpected package name"
[[ "$srcinfo" == *$'provides = amdxdna'* ]] || fail "package does not provide amdxdna"
[[ "$srcinfo" == *$'conflicts = amdxdna-dkms'* ]] || fail "package does not conflict with the obsolete AUR package"

printf 'amdxdna script tests passed\n'
