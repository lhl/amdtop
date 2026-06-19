# amdgpu-top-tui2

An alternate [nvitop](https://github.com/XuehaiPan/nvitop)/[btop](https://github.com/aristocratos/btop)-style
terminal UI for AMD systems — CPU, AMD GPUs (discrete *and* APUs), and the
Strix Halo XDNA NPU — built on top of
[`amdgpu_top`](https://github.com/Umio-Yasuno/amdgpu_top)'s `libamdgpu_top`
backend.

![amdgpu-top-tui2 screenshot](docs/screenshot.png)

> **This is an add-on for [`amdgpu_top`](https://github.com/Umio-Yasuno/amdgpu_top), not a standalone tool.**
> All telemetry comes from `libamdgpu_top` (the same library that powers
> `amdgpu_top`'s `--tui`, `--gui`, and `--json` frontends). Think of this as a
> fourth, alternate TUI frontend — hence `tui2` — focused on the nvitop/btop
> aesthetic. Thanks [@Umio-Yasuno](https://github.com/Umio-Yasuno) for
> the backend that does all the hard work.

## Why

`nvitop` is great but NVIDIA-only. On AMD — especially a Strix Halo box with an
APU **and** an XDNA NPU, or a workstation with multiple AMD cards there was no 
single TUI with that look-and-feel. This fills that gap by rendering 
`libamdgpu_top`'s data in a modern, themeable layout.

## Features

- **Collapsible CPU / GPU / NPU / Processes sections** (state persists across runs)
- **CPU section, btop-style**: per-core grid with braille history mini-graphs,
  package temp/power, load average, plus system MEM/SWP
- **Multi-GPU**: one band per device, labeled by index + PCI bus-id
- **APU-aware memory**: shows the real GTT (unified system RAM) pool on APUs,
  VRAM on discrete cards — both labeled `MEM`
- **XDNA NPU**: presence detection on `/sys/class/accel`; utilization + per-context table when the `amdxdna` driver exposes DRM fdinfo telemetry
- **Process table**: per-process VRAM/GTT and engine usage via `fdinfo`
- **All [btop](https://github.com/aristocratos/btop) themes supported** — drops
  into the same `.theme` files; cycle live with `t`/`T`. Defaults to `onedark`.
- nvitop-style fixed-track gauges with aligned numeric columns; braille area
  graphs with theme-gradient fills

## Install

Requires a **Rust toolchain** and **`libdrm` development headers**, plus an AMD
GPU/APU running the `amdgpu` kernel driver.

```sh
cargo install --git https://github.com/lhl/amdgpu_top_tui2
```

This installs the `amdgpu-top-tui2` binary. The `libamdgpu_top` backend is
pulled in and compiled automatically (statically linked) — you do **not** need
the `amdgpu_top` binary installed separately. The only runtime dependency is
`libdrm_amdgpu.so.1`, which is present on any system with AMD drivers.

Distro packages for the `libdrm` build headers:

| Distro | Package |
|---|---|
| Arch | `libdrm` |
| Debian/Ubuntu | `libdrm-dev` |
| Fedora | `libdrm-devel` |

### NPU telemetry requirements

`amdgpu-top-tui2` detects AMD XDNA/Ryzen AI NPUs through the Linux accel
class (`/sys/class/accel` and `/dev/accel/accel*`). On systems like Strix Halo,
that is enough for the NPU pane to show the device name, firmware, and BDF.

Live NPU utilization and the per-process/context table require extra telemetry
from the `amdxdna` kernel driver via DRM fdinfo. If your driver exposes fdinfo,
opening `/dev/accel/accel0` should show `drm-*` fields such as
`drm-driver`, `drm-pdev`, and `drm-engine-npu-amdxdna`:

```sh
python3 - <<'PY'
import os
fd = os.open('/dev/accel/accel0', os.O_RDWR)
print(open(f'/proc/self/fdinfo/{fd}').read())
PY
```

Expected fdinfo-capable output includes lines similar to:

```text
drm-driver: amdxdna_accel_driver
drm-pdev: 0000:c4:00.1
drm-engine-npu-amdxdna: 0 ns
drm-total-memory: 0 KiB
```

If those `drm-*` lines are missing, the app still shows the NPU pane, but the
utilization gauge is `N/A` and the pane reports `amdxdna fdinfo telemetry
unavailable`. This means the NPU is present, but the loaded kernel module does
not provide the standard fdinfo counters that this TUI reads.

#### Arch / AUR notes

Recent Arch kernels include the in-tree `amdxdna` driver, but not every kernel
build has the fdinfo patches needed for monitoring. For Strix Halo testing, the
most relevant AUR package is:

```sh
yay -S amdxdna-dkms
```

`amdxdna-dkms` provides a newer DKMS `amdxdna` module and firmware, and its
upstream lists Strix Halo (`17f0:11`) support. Make sure the matching kernel
headers for your booted kernel are installed (`linux-headers`,
`linux-mainline-headers`, etc.). Reboot after installing, then run the fdinfo
check above.

Arch also packages the XRT userspace plugin:

```sh
sudo pacman -S xrt-plugin-amdxdna
```

That package is useful for XRT workloads and tools such as `xrt-smi`, but it is
not a replacement for an fdinfo-capable kernel module. The older AUR
`amdxdna-driver-bin` / `xrt-npu-git` packages exist, but appear to target older
XDNA stacks; prefer `amdxdna-dkms` for the kernel driver unless you specifically
need to test that older stack.

> **Note:** `cargo install` from crates.io is not currently possible because we
> depend on `libamdgpu_top` via git (it is not published to crates.io). Prebuilt
> release binaries / AUR packaging are planned.

## Usage

```sh
amdgpu-top-tui2
```

### Keybindings

| Key | Action |
|---|---|
| `q` / `Esc` | quit |
| `Tab` / `Shift+Tab` | move between sections |
| `Space` / `Enter` | collapse / expand the focused section |
| `t` / `T` | next / previous theme |
| `b` / `B` | next / previous gauge block style |

Section collapse state, the selected theme, and the gauge block style all
persist across runs.

### Gauge block styles

Cycle the bar fill glyph with `b`/`B`:
`3/4` (▊, default), `smooth` (precise fractional █), `dotmatrix` (⣿ LED cell),
`lines` (━/─), `squares` (■/□), `rects` (▮/▯), `pills` (▰/▱).

## Themes

Themes are read from the standard btop locations (first match wins):

```
$XDG_CONFIG_HOME/btop/themes/
~/.config/btop/themes/
/usr/local/share/btop/themes/
/usr/share/btop/themes/
```

Any btop `.theme` file works (hex `#RRGGBB`, 2-char grayscale `#BW`, and
`R G B` decimal color formats are all supported, including gradients). The
default is `onedark`; a minimal everforest fallback is bundled in case no theme
files are installed.

## Credits

- Backend: [`libamdgpu_top`](https://github.com/Umio-Yasuno/amdgpu_top) by Umio-Yasuno
- Inspiration: [nvitop](https://github.com/XuehaiPan/nvitop), [btop](https://github.com/aristocratos/btop)

## License

MIT
