# Screenshot Generation Runbook

The canonical `docs/screenshot.png` is captured through the real Ghostty
renderer with `scripts/generate_ghostty_screenshot.py`. This preserves the
terminal's actual line height and its native Braille, block, shading, and
box-drawing sprites instead of approximating them with Pillow font glyphs.

The older `scripts/generate_screenshot.py` pseudo-terminal/Pillow renderer is
retained as a portable layout and text-dump fallback. It is not the canonical
image path.

## Canonical presentation

| Setting | Default |
|---|---:|
| Terminal geometry | 120 columns × 48 rows |
| Terminal | Ghostty |
| Font | JetBrainsMono Nerd Font Mono, 14 pt |
| Theme | Tokyo Night |
| Gauge style | `3/4` |
| Window decoration/padding | disabled / zero |
| Capture duration | 13 seconds |
| CPU load | four affinity-spread workers |
| GPU load | repeating 20/45/70/100% BF16 matrix-multiply duty sweep |
| Output | `docs/screenshot.png` |

On the maintainer's current 1.25× niri output, the 120 × 48 Ghostty surface is
captured as a 1650 × 1478 RGBA PNG. Pixel dimensions may differ with compositor
scale, but the script fails unless the actual child PTY is exactly 120 × 48.

## Why Ghostty and niri are used

Ghostty renders terminal graphics as cell-bounded sprites. Ordinary font
rendering can omit Braille or let full-block glyphs overlap adjacent lines.
The generator launches a dedicated undecorated Ghostty window, asks niri to
float it, reads the child PTY's real dimensions, and resizes proportionally
until the requested grid is exact. It then captures that specific window by ID.

Every run uses a unique temporary `XDG_CONFIG_HOME`, with all amdtop sections
expanded. Before capture, the script verifies the state still has every section
expanded and the requested theme selected. After niri finishes writing the PNG,
the script terminates amdtop and waits for Ghostty to close naturally. Direct
window closure is only an error-cleanup fallback, avoiding Ghostty's warning
about killing a running process.

## Requirements

Run on the Arch/niri workstation used for release screenshots. Required tools:

- `ghostty` and `niri` on an active niri Wayland session;
- `JetBrainsMono Nerd Font Mono`;
- Rust/Cargo plus amdtop's normal native build dependencies;
- `mamba` environment `therock` with ROCm-enabled PyTorch for the synthetic GPU
  sweep.

Verify the relevant environment before capture:

```sh
ghostty +show-face --string='A░▊█⠀⣀⣿─│╭╮'
mamba run -n therock python - <<'PY'
import torch
print(torch.__version__, torch.cuda.is_available())
PY
```

Ghostty should report JetBrains Mono for text and internal sprites for terminal
graphics. PyTorch must report a usable ROCm/CUDA device.

## Generate the canonical screenshot

From the repository root:

```sh
python3 scripts/generate_ghostty_screenshot.py
```

The generator builds `target/screenshot/debug/amdtop` with `Cargo.lock`, starts
bounded CPU and GPU workloads, captures the mature window, replaces
`docs/screenshot.png` atomically, and cleans up every child process.

Useful development variants:

```sh
# Reuse the existing isolated binary and write a temporary preview.
python3 scripts/generate_ghostty_screenshot.py \
  --no-build \
  --capture-seconds 8 \
  --output /tmp/amdtop-preview.png

# Inspect natural system activity without the synthetic GPU sweep.
python3 scripts/generate_ghostty_screenshot.py \
  --no-gpu-load \
  --cpu-load-cores none \
  --output /tmp/amdtop-idle.png
```

The deterministic GPU workload is `scripts/gpu_load.py`. It sets its Linux
process name to `amdtop-gpu-load`, allocates bounded BF16 matrices, and repeats
20/45/70/100% duty phases. The sweep is intended to leave a multicolor GPU
Braille history rather than a flat full-utilization graph. It has a hard runtime
and is always terminated during generator cleanup.

## Controls

Run `python3 scripts/generate_ghostty_screenshot.py --help` for all options.
The main controls are:

- `--columns` / `--rows`: required terminal grid;
- `--font-family` / `--font-size`: Ghostty font selection;
- `--theme` / `--block-style`: isolated amdtop state;
- `--capture-seconds`: history maturity;
- `--cpu-load-cores` / `--cpu-load-workers`: deterministic CPU activity;
- `--gpu-load` / `--no-gpu-load`, `--gpu-load-env`, and
  `--gpu-matrix-size`: synthetic ROCm workload;
- `--target-dir`, `--binary`, and `--no-build`: isolated build selection;
- `--output`: atomic PNG destination.

## Portable fallback and text inspection

For development outside Ghostty+niri, install the pinned Pillow/pyte
requirements in a disposable environment:

```sh
python3 -m venv /tmp/amdtop-screenshot-venv
/tmp/amdtop-screenshot-venv/bin/python -m pip install \
  --requirement scripts/requirements-screenshot.txt
```

Then use the legacy renderer for a text dump or approximate preview:

```sh
/tmp/amdtop-screenshot-venv/bin/python \
  scripts/generate_screenshot.py \
  --columns 120 \
  --rows 48 \
  --output /tmp/amdtop-portable.png \
  --dump-text /tmp/amdtop-screenshot.txt
```

Do not replace the canonical image with this output without explicitly deciding
to return to font-based terminal-graphics rendering.

## Review checklist

Do not commit a generated screenshot without reviewing it.

- [ ] Confirm the generator built the intended amdtop revision.
- [ ] Confirm it reports an exact 120 × 48 Ghostty grid and the expected font.
- [ ] Inspect every section for clipped labels, borders, columns, and footer
      controls.
- [ ] Confirm CPU and GPU Braille histories show useful activity and gradient
      colors.
- [ ] Confirm MEM/SWP blocks remain inside their own rows.
- [ ] Confirm all four sections are expanded.
- [ ] Confirm process names contain no private or workload-specific data beyond
      the intentional `amdtop-gpu-load` helper.
- [ ] Confirm the image contains no hostname, username, private path, model
      name, logs, or unrelated sensitive process name.
- [ ] Confirm the PNG mode and dimensions:

  ```sh
  file docs/screenshot.png
  python3 - <<'PY'
  from PIL import Image
  image = Image.open('docs/screenshot.png')
  print(image.mode, image.size)
  PY
  ```

- [ ] Run the helper and repository checks:

  ```sh
  python3 -m unittest \
    scripts.test_generate_screenshot \
    scripts.test_generate_ghostty_screenshot \
    scripts.test_gpu_load
  cargo fmt --all --check
  cargo clippy --locked --all-targets --all-features -- -D warnings
  cargo test --locked --all-targets --all-features
  git diff --check
  ```

- [ ] Review `git diff --stat`, `git status -sb`, and the rendered PNG before
      staging explicit files.

Screenshot generation remains hardware-, compositor-, and font-dependent and
does not run in CI. Unit tests cover deterministic geometry, state validation,
window selection, and GPU duty scheduling; release review covers the actual
rendering and process lifecycle.
