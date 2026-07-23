#!/usr/bin/env bash
set -euo pipefail

expected_srcversion=2BDE8A5031628D336348E6C
expected_archive_sha256=57384f0d5cf3fc604b00540ed9b7542826cd961c30a2f3207357c972cc3ef32c
expected_firmware_sha256=3e3c996ef1e562e96ee4c4d90faa9faf1132c72da3af1bcf35d592cc34903fed
archive=/usr/share/xrt/amdxdna/bins/xrt_smi_strx.a
firmware=/usr/lib/firmware/amdnpu/17f0_11/1.7_npu.sbin.1.1.2.65
runs=1
print_workload=false

usage() {
  cat <<'EOF'
Usage: scripts/test-amdxdna-npu.sh [--runs <count>] [--print-workload]

Validate the pinned amdxdna DKMS module, fdinfo busy-time counter, firmware,
XRT validation data, and a real GEMM workload. Run as the desktop user after a
reboot or fresh login so the PAM memlock limit has taken effect.

Options:
  --runs <count>     Number of GEMM validations to execute (default: 1)
  --print-workload   Print a sustained workload command without touching hardware
  -h, --help         Show this help
EOF
}

while (($#)); do
  case "$1" in
    --runs)
      (($# >= 2)) || { printf '%s\n' '--runs requires a value' >&2; exit 2; }
      runs=$2
      shift
      ;;
    --print-workload) print_workload=true ;;
    -h|--help) usage; exit 0 ;;
    *) printf 'unknown option: %s\n' "$1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

[[ "$runs" =~ ^[1-9][0-9]*$ ]] || {
  printf 'run count must be a positive integer: %s\n' "$runs" >&2
  exit 2
}

if $print_workload; then
  sequence=$(seq -s ' ' 1 "$runs")
  printf 'for _ in %s; do xrt-smi validate --run gemm --batch || break; done\n' "$sequence"
  exit 0
fi

((EUID != 0)) || {
  printf 'run this test as the desktop user, not root, so amdtop sees the same-user fdinfo client\n' >&2
  exit 1
}

for command in modinfo python3 sha256sum xrt-smi; do
  command -v "$command" >/dev/null || {
    printf 'missing required command: %s\n' "$command" >&2
    exit 1
  }
done

selected_module=$(modinfo -n amdxdna)
selected_srcversion=$(modinfo -F srcversion amdxdna)
loaded_srcversion=$(< /sys/module/amdxdna/srcversion)
memlock=$(ulimit -l)

printf 'kernel:              %s\n' "$(uname -r)"
printf 'selected module:     %s\n' "$selected_module"
printf 'selected srcversion: %s\n' "$selected_srcversion"
printf 'loaded srcversion:   %s\n' "$loaded_srcversion"
printf 'memlock (KiB):       %s\n' "$memlock"

[[ "$selected_module" == */updates/dkms/amdxdna.ko* ]] || {
  printf 'selected module is not the DKMS build\n' >&2
  exit 1
}
[[ "$selected_srcversion" == "$expected_srcversion" ]] || {
  printf 'selected module does not match the pinned test build\n' >&2
  exit 1
}
[[ "$loaded_srcversion" == "$expected_srcversion" ]] || {
  printf 'loaded module differs from the selected DKMS build; reboot or reload it\n' >&2
  exit 1
}
[[ "$memlock" == unlimited ]] || {
  printf 'memlock is not unlimited; start a fresh login session or reboot\n' >&2
  exit 1
}
[[ -r /dev/accel/accel0 && -w /dev/accel/accel0 ]] || {
  printf '/dev/accel/accel0 is not accessible to this user\n' >&2
  exit 1
}

echo "$expected_archive_sha256  $archive" | sha256sum --check --status || {
  printf 'missing or unexpected XRT Strix validation archive: %s\n' "$archive" >&2
  exit 1
}
echo "$expected_firmware_sha256  $firmware" | sha256sum --check --status || {
  printf 'missing or unexpected pinned NPU firmware: %s\n' "$firmware" >&2
  exit 1
}

python3 - <<'PY'
import os

fd = os.open('/dev/accel/accel0', os.O_RDWR)
text = open(f'/proc/self/fdinfo/{fd}').read()
os.close(fd)
engine = next((line for line in text.splitlines() if line.startswith('drm-engine-')), None)
if engine is None:
    raise SystemExit('missing drm-engine-* fdinfo counter')
print(f'fdinfo counter:      {engine}')
if not engine.startswith('drm-engine-npu-amdxdna:'):
    raise SystemExit(f'unexpected engine key: {engine}')
PY

xrt-smi examine
for ((run = 1; run <= runs; run++)); do
  printf '\nGEMM validation %d/%d\n' "$run" "$runs"
  xrt-smi validate --run gemm --batch
done

printf '\nNPU stack check passed. For a sustained amdtop workload run:\n'
sequence=$(seq -s ' ' 1 100)
printf '  for _ in %s; do xrt-smi validate --run gemm --batch || break; done\n' "$sequence"
