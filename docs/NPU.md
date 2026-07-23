# AMD XDNA NPU telemetry and workload testing

> [!IMPORTANT]
> XDNA support on Linux is still moving quickly. A system can detect and use an
> NPU without exposing the per-client busy-time counter that `amdtop` needs.
> Driver, firmware, XRT, and the XRT XDNA plugin should be treated as a matched
> stack. Package names and kernel compatibility can change between releases.
>
> This guide records the state investigated on **2026-07-23**. Recheck the
> linked upstream sources before replacing a working driver.

## Quick answer

To show live NPU utilization, `amdtop` needs all of the following:

1. An AMD XDNA device bound to `amdxdna`, normally visible as
   `/dev/accel/accel0`.
2. An `amdxdna` kernel module that exports a cumulative per-client
   `drm-engine-*` busy-time counter through `/proc/<pid>/fdinfo/<fd>`.
3. An `amdtop`/`libamdgpu_top` version that recognizes the driver's engine key.
4. A real NPU workload. The easiest test workload is normally an XRT validation
   test, not a hand-written application.

Once the stack is working, use two terminals:

```sh
# Terminal 1
amdtop
```

```sh
# Terminal 2 (repeat until Ctrl-C)
while xrt-smi validate --run gemm; do :; done
```

The set of validation tests is device- and XRT-version-dependent. Check
`xrt-smi validate --help`; if `gemm` or `--loop` is unavailable, run
`xrt-smi validate` or repeat another listed compute test.

**Do not start by writing a test application.** An XDNA workload needs a
compiled array overlay and control code in addition to ordinary host code. XRT
already ships validation infrastructure and suitable test artifacts. The Linux
[kernel XDNA documentation][kernel-amdxdna] describes the workload binary and
execution model.

## What `amdtop` measures

Linux defines `drm-engine-<name>: <value> ns` as the cumulative time an engine
spent executing work belonging to a DRM client. Monitoring tools sample the
counter, subtract the previous sample, and divide by elapsed wall-clock time.
See the kernel's [DRM client usage statistics specification][drm-usage-stats].

For XDNA, this means:

- the data is per open DRM/accel client, which usually maps to an application
  context;
- the reported percentage is **busy time**, not percentage of peak TOPS,
  frequency, tile occupancy, model efficiency, or memory bandwidth;
- `amdtop` aggregates active XDNA clients and clamps the displayed total to
  100%; and
- a short workload may finish before the next sampling interval, so a repeated
  validation test is easier to observe.

Device name, firmware version, and PCI BDF come from sysfs. They may appear even
when utilization is unavailable because presence detection and busy-time
telemetry are separate capabilities.

## Understand the stack

| Layer | Typical component | What it provides |
|---|---|---|
| Hardware | Ryzen AI/XDNA NPU | The accelerator |
| Kernel | `amdxdna.ko` | `/dev/accel/accel*`, contexts, execution, fdinfo |
| Firmware | `amdnpu/<device>/npu*.sbin` | NPU microcontroller firmware matched to the driver |
| Userspace runtime | XRT | `xrt-smi` and runtime APIs |
| Device plugin | XRT XDNA shim | Connects XRT to `amdxdna` |
| Workload | XRT validation or an inference app | Submits actual NPU jobs |
| Monitor | `amdtop` + `libamdgpu_top` | Samples and displays fdinfo counters |

Installing only an XRT plugin does not add kernel fdinfo counters. Conversely,
a telemetry-capable kernel module does not provide an NPU workload or guarantee
that an older XRT/plugin can use its ioctls.

## Diagnose before changing anything

### 1. Confirm device detection and driver binding

```sh
uname -r
lspci -nnk | grep -A3 -Ei 'signal processing|neural|17f0|1502'
ls -l /sys/class/accel /dev/accel
readlink -f /sys/class/accel/accel0/device/driver
```

Expected results include an accel device and a driver path ending in
`/drivers/amdxdna`. Absence of `/dev/accel/accel*` is a driver/device problem,
not an `amdtop` telemetry problem.

### 2. Inspect the fdinfo ABI directly

Opening a fresh accel client is enough to determine whether the loaded driver
advertises an engine counter:

```sh
python3 - <<'PY'
import glob
import os

paths = sorted(glob.glob('/dev/accel/accel*'))
if not paths:
    raise SystemExit('no /dev/accel/accel* device found')

path = paths[0]
fd = os.open(path, os.O_RDWR)
print(f'# {path}')
print(open(f'/proc/self/fdinfo/{fd}').read(), end='')
PY
```

A telemetry-capable result must contain `drm-driver`, `drm-pdev`, and a
`drm-engine-*` line. Two XDNA implementations currently use different engine
names:

```text
# AMD legacy/out-of-tree driver
# (recognized by libamdgpu_top 0.11.5)
drm-engine-npu-amdxdna: 0 ns
```

```text
# Current AMD upstream/staging tree after commit a87f856
drm-engine-amdxdna_accel_driver: 0 ns
```

Memory lines such as these are useful, but **do not provide utilization**:

```text
drm-total-memory: 0 KiB
drm-shared-memory: 0 KiB
```

`drm-driver` by itself proves only that DRM fdinfo is active. It does not prove
that engine busy-time accounting is implemented.

### 3. Identify the module that is actually installed and selected

```sh
modinfo -n amdxdna
modinfo amdxdna | grep -E '^(filename|version|srcversion|vermagic|intree):'
dkms status | grep -i amdxdna || true
```

On a package-managed distribution, also ask which package owns the selected
module. For example, on Arch:

```sh
pacman -Qo "$(modinfo -n amdxdna)"
```

Useful source-level checks are:

```sh
grep -RIn 'drm-engine-' /usr/src/amdxdna-* 2>/dev/null || true
grep -RIn 'show_fdinfo' /usr/src/amdxdna-* 2>/dev/null || true
```

An installed `*-dkms` package is not proof that its module built for the booted
kernel or that it replaced the in-tree module. `dkms status` must list the
booted kernel, `modinfo -n` must resolve to the intended module, and the fdinfo
probe remains the final test. Reboot after changing module packages; otherwise
the old module may remain loaded.

## Current compatibility caveat

At the time of writing, there are three materially different combinations:

| Driver source | Engine output | `libamdgpu_top` 0.11.5 |
|---|---|---|
| Linux v7.2-rc1 in-tree `amdxdna` | No `drm-engine-*` line | No utilization possible |
| AMD upstream/staging tree at or after [`a87f856`][busy-time-commit] | `drm-engine-amdxdna_accel_driver` | Counter is currently not recognized |
| AMD legacy/out-of-tree tree | `drm-engine-npu-amdxdna` | Recognized |

The upstream/staging busy-time patch was added to AMD's repository as
[`accel/amdxdna: Expose NPU device busy time via DRM fdinfo`][busy-time-commit].
Linux v7.2-rc1's [`amdxdna_show_fdinfo()`][linux-v7.2-fdinfo] reports memory
statistics but not an engine counter. The AMD repository's current
[staging implementation][amd-staging-fdinfo] includes busy-time accounting,
while its [legacy implementation][amd-legacy-fdinfo] uses the older engine
name.

`amdtop` currently pins `libamdgpu_top` 0.11.5. That version's
[XDNA fdinfo parser][libamdgpu-top-parser] accepts
`drm-engine-npu-amdxdna` specifically. The preferred long-term fix is for the
parser to accept the valid engine name exported by the upstream/staging driver
(or robustly parse XDNA `drm-engine-*` keys). Until that lands, the legacy driver
is the shortest path to testing the released parser, while the staging driver
is the better path for testing the future stack.

There is also a current UI-detection limitation: `amdtop` uses the presence of
`drm-driver` as its initial fdinfo capability probe. A driver that exports DRM
memory statistics but no recognized engine counter can therefore appear as a
0% gauge instead of `N/A`. Trust the direct `drm-engine-*` check above when
diagnosing telemetry.

## Recommended installation strategy

Use this order of preference:

1. **Use a distribution kernel/package that already contains busy-time
   accounting.** This will eventually be the easiest and safest route. Verify
   the actual fdinfo output rather than relying on a package version.
2. **For current development, build a matched stack from
   [`amd/xdna-driver`][xdna-driver].** Build XRT, the XDNA plugin, driver, and
   firmware from one checkout. This is the most general reproducible route.
3. **For a quick compatibility experiment, package the legacy driver from that
   same checkout.** This emits the key recognized by `libamdgpu_top` 0.11.5,
   but it is maintained primarily for compatibility and bring-up.
4. **Avoid a driver-only replacement unless you are prepared to recover from
   driver/firmware/plugin mismatches.** A standalone module can compile and
   still fail at runtime.

Before changing the stack, record the working state and keep a known-good kernel
in the boot menu:

```sh
uname -a
modinfo amdxdna > amdxdna-modinfo.before.txt
xrt-smi examine > xrt-examine.before.txt 2>&1 || true
journalctl -k -b | grep -i amdxdna > amdxdna-kernel.before.txt || true
```

### Why a matched build is recommended

The AMD driver README explicitly recommends building XRT from the repository's
submodule so that XRT and its plugin match. It also documents two common
mismatch failures:

- stale firmware can cause `ERT_CMD_STATE_ABORT` or mailbox timeouts; and
- a newer XRT/plugin with an older kernel driver can produce `EOPNOTSUPP` or
  `EINVAL` for newer queries and ioctls.

See AMD's [build, test, and troubleshooting instructions][xdna-driver-readme].

## Build the AMD stack

The following is a guide to AMD's upstream procedure, not a promise that every
moving kernel/checkout combination will compile. Kernel APIs change, so pin and
record the checkout used for a working build.

### Common preparation

Install the headers for the **exact kernel you plan to boot**. Confirm the
kernel configuration includes DRM accel and AMD IOMMU support:

```sh
kernel=$(uname -r)
test -e "/lib/modules/$kernel/build/Makefile" || echo "matching headers missing"

if test -r /proc/config.gz; then
    zgrep -E 'CONFIG_(DRM_ACCEL|AMD_IOMMU)=' /proc/config.gz
else
    grep -E 'CONFIG_(DRM_ACCEL|AMD_IOMMU)=' "/boot/config-$kernel"
fi
```

Clone the complete repository and record its commit:

```sh
git clone --recurse-submodules https://github.com/amd/xdna-driver.git
cd xdna-driver
git rev-parse HEAD
```

Confirm the checkout contains the staging busy-time patch:

```sh
git merge-base --is-ancestor \
    a87f8566320e4b8f1cda87b328dd58e90df4f13e HEAD
echo $?   # 0 means the commit is an ancestor
```

AMD provides a dependency installer:

```sh
sudo ./tools/amdxdna_deps.sh
```

Review that script before running it. It invokes XRT's distribution dependency
installer and may install many build packages.

### Arch Linux

AMD documents building XRT base/NPU packages first, followed by a separate DKMS
driver package and XDNA plugin package:

```sh
# From the xdna-driver repository root
cd xrt/build
./build.sh -npu -opt

cd arch
makepkg -p PKGBUILD-xrt-base
sudo pacman -U ./xrt-base-*.pkg.tar.zst

makepkg -p PKGBUILD-xrt-npu
sudo pacman -U ./xrt-npu-*.pkg.tar.zst

cd ../../../build
./build.sh -release -package_upstream_driver

cd arch
makepkg -p PKGBUILD-amdxdna-driver
sudo pacman -U ./amdxdna-driver-*.pkg.tar.zst

makepkg -p PKGBUILD-xrt-plugin-amdxdna
sudo pacman -U ./xrt-plugin-amdxdna-*.pkg.tar.zst
```

The exact output names follow the checkout's package version. Inspect files
before using broad globs with `sudo pacman -U`.

The generated `PKGBUILD-amdxdna-driver` currently depends on the stock
`linux-headers` package. If using `linux-mainline`, Zen, LTS, or another custom
kernel, install its matching headers and adjust the local package dependency if
necessary. DKMS still has to build separately for each installed kernel.

Do not silently mix Arch's `xrt`/`xrt-plugin-amdxdna` packages with a different
checkout's custom XRT, plugin, driver, and firmware. Review conflicts and
package ownership first:

```sh
pacman -Q | grep -E '^(xrt|xrt-|amdxdna)'
pacman -Qo /usr/bin/xrt-smi 2>/dev/null || true
```

#### Short-term legacy-driver build

To emit `drm-engine-npu-amdxdna`, package AMD's legacy driver instead of the
default staging driver:

```sh
# From the xdna-driver repository root
cd build
./build.sh -release -package_legacy_driver
```

Then create/install the Arch driver and plugin packages as above. This changes
which source tree is packaged as the primary `amdxdna.ko`; it does not mean that
two modules should be loaded simultaneously. The build flags are defined in
AMD's [`build/build.sh`][xdna-build-script].

Use the legacy route only as a compatibility bridge. For the long-term default
staging route, update `libamdgpu_top` to recognize
`drm-engine-amdxdna_accel_driver`.

### Ubuntu/Debian

AMD's documented matched-build flow is:

```sh
# From the xdna-driver repository root
cd xrt/build
./build.sh -npu -opt
sudo apt install ./Release/xrt_*-base.deb

cd ../../build
./build.sh -release -package_upstream_driver
sudo apt install ./Release/xrt_plugin.*-amdxdna.deb
```

Adapt package names to the generated files. According to the AMD README, the
plugin DEB includes the XDNA shim, DKMS source, and matching firmware; XRT base
is installed separately. For the current released `libamdgpu_top` parser, use
`-package_legacy_driver` instead of `-package_upstream_driver` as a temporary
compatibility experiment.

AMD's `amdxdna-dkms` PPA packaging is another option on the Ubuntu kernels it
explicitly supports, but verify that its source includes the busy-time patch.
Device support alone does not imply fdinfo engine accounting.

### Advanced: build only the staging kernel module

The current AMD tree can build its staging driver against matching installed
kernel headers without building all of XRT. This is useful as a compile test or
for driver developers:

```sh
# From the xdna-driver repository root
kernel=$(uname -r)

KERNEL_VER="$kernel" \
OUT=drivers/accel/amdxdna/config_kernel.h \
bash drivers/accel/tools/configure_kernel.sh

make -f drivers/accel/amdxdna/Makefile \
    BUILD_ROOT_DIR="$PWD/build-kmod" \
    KERNEL_VER="$kernel" \
    XDNA_BUS_TYPE=pci \
    -j"$(nproc)"
```

This was compile-tested against `7.2.0-rc1-1-mainline` during preparation of
this guide. It was **not installed or runtime-tested**. Prefer the generated
DKMS/distribution packages for installation, upgrades, initramfs handling, and
rollback. A newly compiled module may require newer matching firmware or XRT
plugin ioctls; blindly installing it over a distribution stack can leave the
NPU unusable.

## Configure runtime access

### Locked-memory limit

XDNA contexts use a 64 MiB host-resident instruction buffer, as documented by
the kernel. A typical 8 MiB `RLIMIT_MEMLOCK` can make `xrt-smi` fail with an
error similar to:

```text
mmap(... len=67108864 ...) failed: Resource temporarily unavailable
```

AMD recommends raising the login-session limit:

```sh
sudo mkdir -p /etc/security/limits.d
sudo tee /etc/security/limits.d/99-amdxdna.conf >/dev/null <<'EOF'
* soft memlock unlimited
* hard memlock unlimited
EOF
```

Log out completely and back in, or reboot, then check:

```sh
ulimit -l
```

Expected output is `unlimited`. PAM limits do not automatically alter an
already-running shell. For a systemd service, set an appropriate
`LimitMEMLOCK=` in the service instead.

### Device permissions

The user running workloads needs read/write access to the accel device:

```sh
ls -l /dev/accel/accel*
test -r /dev/accel/accel0 && test -w /dev/accel/accel0 && echo accessible
```

Many distributions grant access through the `render` group. If required:

```sh
sudo usermod -aG render "$USER"
```

Log out and back in after changing groups. Prefer a distribution udev rule over
making the device globally writable by hand.

## Verify after installation

Reboot into the intended kernel, then verify each layer in order.

### 1. Module and firmware

```sh
uname -r
modinfo -n amdxdna
modinfo amdxdna | grep -E '^(filename|version|vermagic|intree):'
dkms status | grep -i amdxdna || true
journalctl -k -b | grep -iE 'amdxdna|amdnpu'
```

### 2. Engine counter

Repeat the Python fdinfo probe. Stop here if no `drm-engine-*` line appears;
no workload can create a counter that the loaded driver does not implement.

### 3. XRT runtime

On an `/opt/xilinx/xrt` installation, initialize the environment first:

```sh
source /opt/xilinx/xrt/setup.sh
```

Distribution packages may install XRT directly under `/usr` and need no setup
script. Then run:

```sh
xrt-smi examine
xrt-smi validate
```

### 4. Sustained observable workload

```sh
xrt-smi validate --help
while xrt-smi validate --run gemm; do :; done
```

XRT's [`gemm` validation test][xrt-gemm-test] executes INT8 GEMM on supported
NPU platforms. The validate command and available tests evolve, so use a
listed compute-heavy test if `gemm` is absent. Some versions support a native
loop option instead:

```sh
xrt-smi validate --run gemm --loop 100
```

Use only options shown by the installed version's help. Stop the shell loop
with `Ctrl-C`.

While it runs, inspect the workload's own accel fdinfo:

```sh
pid=$(pgrep -n xrt-smi)
for info in /proc/"$pid"/fdinfo/*; do
    grep -H -E '^drm-(driver|pdev|client-id|engine-)' "$info" 2>/dev/null || true
done
```

The engine value should increase between reads. `/proc` mount options such as
`hidepid` can prevent reading another process's fdinfo; run the probe as the
same user or with appropriate privileges.

Finally, run `amdtop`. A compatible parser should show the `xrt-smi` context and
non-zero utilization.

## Arch/AUR status as of 2026-07-23

Arch provides official [`xrt`][arch-xrt] and
[`xrt-plugin-amdxdna`][arch-xrt-plugin] packages. These supply the userspace
runtime and plugin; they do not guarantee that the booted kernel's `amdxdna`
module has fdinfo busy-time accounting.

Arch's `xrt-plugin-amdxdna` 2.21.75-2 build recipe also removes the packaged
`bins` directory. This omits the VTD validation archive that `xrt-smi validate`
needs. The resulting failure is:

```text
Error(s) : No archive provided, skipping test
```

For that exact plugin version, AMD's 2.21.75 source pins the Strix archive to
Xilinx/VTD commit `c79b5d2`. The plugin searches for it at:

```text
/usr/share/xrt/amdxdna/bins/xrt_smi_strx.a
```

Prefer building/installing AMD's matched plugin package so the archive is
package-managed. If repairing the distribution package locally, use the VTD
revision pinned by the installed plugin's source rather than an arbitrary
latest archive. The 2.21.75 Strix file has SHA-256
`57384f0d5cf3fc604b00540ed9b7542826cd961c30a2f3207357c972cc3ef32c`.
This validation-data issue is independent of the kernel driver and fdinfo
counter.

The AUR [`amdxdna-dkms`][aur-amdxdna] package examined for this guide:

- packages driver source without the `drm-engine-*` busy-time accounting added
  by AMD's commit `a87f856`;
- has a [`BUILD_EXCLUSIVE_KERNEL` rule][aur-dkms-conf] matching only Linux
  6.17, 6.18, and 6.19; and
- therefore will not build on kernels outside that expression without package
  changes.

Consequently, `yay -S amdxdna-dkms` may install source under `/usr/src` while
`dkms status` contains no `amdxdna` entry and the in-tree module remains
selected. Always verify the loaded result. The package remains potentially
useful for the kernels and device support it targets, but it is not currently a
general fdinfo-utilization solution.

## Troubleshooting

### NPU appears, but utilization is `N/A` or always 0%

1. Run the direct fdinfo probe.
2. Require a `drm-engine-*` line, not merely `drm-driver`.
3. Confirm the value increases while an NPU workload runs.
4. Check whether the engine name is supported by the installed
   `libamdgpu_top` parser.
5. Confirm the workload lasts longer than one `amdtop` sample.

### DKMS package is installed, but the in-tree module is loaded

```sh
dkms status | grep -i amdxdna || true
modinfo -n amdxdna
modinfo -F intree amdxdna 2>/dev/null || true
```

Check the DKMS build log, kernel-version allowlist, matching headers, and Secure
Boot policy. Rebuild for the exact kernel and reboot. If Secure Boot is enabled,
an unsigned DKMS module may be rejected even after a successful build.

### `xrt-smi` fails with `mmap` / `Resource temporarily unavailable`

Check `ulimit -l`. A 64 MiB locked mapping cannot fit under a common 8 MiB
limit. Apply the memlock configuration above and start a new login session.

### `ERT_CMD_STATE_ABORT` or mailbox timeout

Treat this as a likely firmware/driver mismatch. Use firmware delivered with
the same AMD checkout/package as the driver, rebuild the initramfs if your
distribution requires it, and reboot. Inspect `journalctl -k -b` before trying
more workloads.

### `Operation not supported`, `EOPNOTSUPP`, or `EINVAL`

A newer XRT/plugin may be invoking ioctls absent from the loaded driver. Use a
matched stack rather than independently upgrading only XRT or only the kernel
module.

### `Permission denied` opening `/dev/accel/accel0`

Check device ownership, udev rules, current group membership (`id`), and whether
the session predates a group change.

### Driver has a counter, but `amdtop` has no process row

Check the exact engine key. The current parser mismatch between
`drm-engine-amdxdna_accel_driver` and `drm-engine-npu-amdxdna` is sufficient to
cause this. Also check `/proc` visibility and whether the process exits too
quickly.

## References

- [Linux: DRM client usage statistics][drm-usage-stats]
- [Linux: AMD NPU/amdxdna architecture and userspace model][kernel-amdxdna]
- [Linux v7.2-rc1 `amdxdna_show_fdinfo()`][linux-v7.2-fdinfo]
- [AMD XDNA driver repository][xdna-driver]
- [AMD XDNA driver build and troubleshooting README][xdna-driver-readme]
- [AMD busy-time fdinfo commit `a87f856`][busy-time-commit]
- [AMD staging fdinfo implementation][amd-staging-fdinfo]
- [AMD legacy fdinfo implementation][amd-legacy-fdinfo]
- [AMD build script and driver-selection flags][xdna-build-script]
- [`libamdgpu_top` 0.11.5 XDNA fdinfo parser][libamdgpu-top-parser]
- [XRT GEMM validation test][xrt-gemm-test]
- [Arch `xrt` package][arch-xrt]
- [Arch `xrt-plugin-amdxdna` package][arch-xrt-plugin]
- [Arch `xrt-plugin-amdxdna` build recipe][arch-xrt-plugin-pkgbuild]
- [AMD XDNA 2.21.75 validation-data pins][xdna-2.21.75-info]
- [AUR `amdxdna-dkms` package][aur-amdxdna]
- [AUR `amdxdna-dkms` kernel allowlist][aur-dkms-conf]

[drm-usage-stats]: https://docs.kernel.org/gpu/drm-usage-stats.html
[kernel-amdxdna]: https://docs.kernel.org/accel/amdxdna/amdnpu.html
[linux-v7.2-fdinfo]: https://github.com/torvalds/linux/blob/v7.2-rc1/drivers/accel/amdxdna/amdxdna_pci_drv.c#L276-L310
[xdna-driver]: https://github.com/amd/xdna-driver
[xdna-driver-readme]: https://github.com/amd/xdna-driver#readme
[busy-time-commit]: https://github.com/amd/xdna-driver/commit/a87f8566320e4b8f1cda87b328dd58e90df4f13e
[amd-staging-fdinfo]: https://github.com/amd/xdna-driver/blob/main/drivers/accel/amdxdna/amdxdna_pci_drv.c#L351-L381
[amd-legacy-fdinfo]: https://github.com/amd/xdna-driver/blob/main/src/driver/amdxdna/amdxdna_drm.c#L391-L410
[xdna-build-script]: https://github.com/amd/xdna-driver/blob/main/build/build.sh
[libamdgpu-top-parser]: https://github.com/Umio-Yasuno/amdgpu_top/blob/v0.11.5/crates/libamdgpu_top/src/xdna/xdna_fdinfo.rs#L95-L104
[xrt-gemm-test]: https://github.com/Xilinx/XRT/blob/master/src/runtime_src/core/tools/common/tests/TestGemm.cpp
[arch-xrt]: https://archlinux.org/packages/extra/x86_64/xrt/
[arch-xrt-plugin]: https://archlinux.org/packages/extra/x86_64/xrt-plugin-amdxdna/
[arch-xrt-plugin-pkgbuild]: https://gitlab.archlinux.org/archlinux/packaging/packages/xrt-plugin-amdxdna/-/blob/1-2.21.75-2/PKGBUILD
[xdna-2.21.75-info]: https://github.com/amd/xdna-driver/blob/2.21.75/tools/info.json
[aur-amdxdna]: https://aur.archlinux.org/packages/amdxdna-dkms
[aur-dkms-conf]: https://aur.archlinux.org/cgit/aur.git/plain/dkms.conf?h=amdxdna-dkms
