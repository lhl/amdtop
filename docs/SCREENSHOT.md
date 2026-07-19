# Screenshot Generation Runbook

Use `scripts/generate_screenshot.py` to capture amdtop in a controlled
pseudo-terminal and render `docs/screenshot.png`. Keep the script and this
runbook current when the TUI gains sections, columns, themes, or layout rules.

The generator intentionally lives outside the Rust application. Its Python
packages are maintainer-only dependencies and do not affect the amdtop binary.

## What the Generator Controls

The script:

- cleans and builds `target/screenshot/debug/amdtop` with the lockfile unless
  `--no-build` is used, keeping documentation builds isolated from normal Cargo
  profiles;
- launches it in an isolated pseudo-terminal and temporary `XDG_CONFIG_HOME`;
- expands every section with the `onedark` theme and `3/4` gauge blocks;
- creates short, variable, affinity-pinned CPU load on four available cores by
  default;
- can retain the latest mature frame containing a desired process or label;
- renders terminal colors and attributes into a PNG with Pillow; and
- replaces the output atomically only after capture and rendering succeed.

The defaults reproduce the repository's intended presentation:

| Setting | Default |
|---|---:|
| Terminal geometry | 188 columns × 46 rows |
| Preferred font | Iosevka Term Nerd Font Mono |
| Font size | 19 px |
| Character cell | 10 × 20 px |
| Canvas margin | 4 px |
| Output image | 1888 × 928 px |
| Capture duration | 13 seconds |
| Mature-frame threshold | 8 seconds |
| Output | `docs/screenshot.png` |

Fontconfig resolves font family names and the script prints the exact regular
and bold files used. Pass explicit font paths when Fontconfig is unavailable or
when exact font files must be selected.

## One-Time Setup

Run from the repository root. Use a disposable virtual environment so these
packages do not modify the project or system Python environments:

```sh
python3 -m venv /tmp/amdtop-screenshot-venv
/tmp/amdtop-screenshot-venv/bin/python -m pip install \
  --requirement scripts/requirements-screenshot.txt
```

The pinned requirements are the rendering contract. Update them deliberately,
regenerate to a temporary output, and visually compare the result before
changing `docs/screenshot.png`.

The preferred font must also be installed. To inspect what Fontconfig will use:

```sh
fc-match 'Iosevka Term Nerd Font Mono'
fc-match 'Iosevka Term Nerd Font Mono:style=Bold'
```

A different monospace font can be supplied with `--font` and `--bold-font`.
Choose one with box-drawing, block, and Braille glyph coverage.

## Generate a Screenshot

For a basic capture with variable CPU activity:

```sh
/tmp/amdtop-screenshot-venv/bin/python \
  scripts/generate_screenshot.py \
  --dump-text /tmp/amdtop-screenshot.txt
```

This updates `docs/screenshot.png`. During layout experiments, write elsewhere
so a failed experiment cannot replace the reviewed image:

```sh
/tmp/amdtop-screenshot-venv/bin/python \
  scripts/generate_screenshot.py \
  --output /tmp/amdtop-screenshot.png \
  --dump-text /tmp/amdtop-screenshot.txt
```

The script stops its amdtop child and CPU load workers on success, failure, or
interruption. It does not stop an external GPU workload.

## Capture Representative GPU Process Activity

Start a trusted GPU workload separately before invoking the generator. For the
current dual-GPU screenshot, `llama-bench` was already running in a loop. Do not
put model paths or workload-specific commands in this repository.

Confirm the workload and then require a frame in which amdtop reports it:

```sh
pgrep -a -x llama-bench

/tmp/amdtop-screenshot-venv/bin/python \
  scripts/generate_screenshot.py \
  --prefer-text llama-bench \
  --require-text \
  --dump-text /tmp/amdtop-screenshot.txt
```

`--prefer-text` retains the latest stable frame after the mature-frame threshold
that contains the text. `--require-text` makes absence an error and leaves the
existing PNG untouched. This is useful for workloads that restart with new
PIDs. Change the preferred text when demonstrating another process or feature.

CPU cores are selected from the generator's affinity mask and spread across the
available set. To reproduce a known workstation layout or disable synthetic
CPU load:

```sh
# Explicit cores; each receives a different repeating duty cycle.
/tmp/amdtop-screenshot-venv/bin/python \
  scripts/generate_screenshot.py --cpu-load-cores 0,5,16,23

# Preserve the machine's natural CPU activity.
/tmp/amdtop-screenshot-venv/bin/python \
  scripts/generate_screenshot.py --cpu-load-cores none
```

The explicit list must be valid for the current process affinity mask.

## Adjust Layout and Rendering

Every presentation parameter has a command-line option. For example:

```sh
/tmp/amdtop-screenshot-venv/bin/python \
  scripts/generate_screenshot.py \
  --columns 176 \
  --rows 48 \
  --font 'Iosevka Term Nerd Font Mono' \
  --font-size 18 \
  --cell-width 9 \
  --cell-height 19 \
  --margin 4 \
  --output /tmp/amdtop-layout-test.png
```

Useful controls include:

- `--columns` and `--rows`: terminal layout and section allocation;
- `--font`, `--bold-font`, and `--font-size`: glyph appearance;
- `--cell-width`, `--cell-height`, and `--text-offset-y`: character grid and
  baseline alignment;
- `--theme` and `--block-style`: isolated amdtop presentation state;
- `--capture-seconds` and `--min-candidate-seconds`: history depth and frame
  selection;
- `--default-fg` and `--default-bg`: terminal defaults used by the renderer;
- `--target-dir`: isolated Cargo output used for the canonical build;
- `--binary` and `--no-build`: capture a specific already-built executable;
- `--print-screen` and `--dump-text`: inspect the selected terminal cells.

Run the generator with `--help` for the complete interface. Record intentional
changes to the canonical defaults in the script, this table, and
the changelog together.

## Review Checklist

Do not commit a generated screenshot without reviewing both the image and its
text dump.

- [ ] Confirm the generator built the intended amdtop revision.
- [ ] Confirm the printed terminal geometry, pixel dimensions, and resolved font
      paths are expected.
- [ ] Inspect every section for clipped labels, borders, columns, and footer
      controls.
- [ ] Confirm histories show useful activity without obscuring current values.
- [ ] Confirm the process table contains only intentional, non-sensitive process
      names and realistic values.
- [ ] Check the text dump for hostnames, usernames, private paths, model names,
      logs, or other machine-specific data.
- [ ] Keep enough process-table room for representative rows, but remove
      excessive empty height.
- [ ] Confirm the PNG is RGB and has the expected dimensions:

  ```sh
  file docs/screenshot.png
  python3 - <<'PY'
  from PIL import Image
  image = Image.open('docs/screenshot.png')
  print(image.mode, image.size)
  PY
  ```

- [ ] Run the helper tests and repository checks:

  ```sh
  /tmp/amdtop-screenshot-venv/bin/python -m unittest \
    scripts.test_generate_screenshot
  cargo fmt --all --check
  cargo clippy --locked --all-targets --all-features -- -D warnings
  cargo test --locked --all-targets --all-features
  git diff --check
  ```

- [ ] Review `git diff --stat`, `git status -sb`, and the rendered PNG before
      staging explicit files.

Screenshot generation is hardware- and font-dependent and therefore does not
run in CI. The helper tests cover deterministic color, geometry, and CPU-core
selection behavior; maintainers must perform the final visual review.
