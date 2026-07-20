# Native themes

amdtop themes are versioned TOML files. The 41 shipped themes are compiled into
the executable, so they work with `cargo install amdtop` and do not require
btop or separately installed assets.

## Search order

For a selected theme named `example`, amdtop checks these files in order:

1. `$XDG_CONFIG_HOME/amdtop/themes/example.toml` when `XDG_CONFIG_HOME` is absolute;
2. `~/.config/amdtop/themes/example.toml`;
3. `/usr/local/share/amdtop/themes/example.toml`;
4. `/usr/share/amdtop/themes/example.toml`;
5. the embedded theme named `example`;
6. the embedded `tokyo-night` fallback.

Native files found in these directories are included in the `t`/`T` theme
cycle. Legacy btop `.theme` files and btop theme directories are not read.

## Schema

Every file must set `schema = 1`. All other sections and roles are optional;
omitted values inherit amdtop's Tokyo Night defaults.

```toml
schema = 1
name = "my-theme"

[palette]
bg = "#1f2329"
fg = "#d8dee9"
accent = "#61afef"
muted = "#5c6370"
green = "#98c379"
yellow = "#e5c07b"
red = "#e06c75"

[ui]
background = "$bg"
foreground = "$fg"
title = "$fg"
highlight = "$accent"
selected_background = "#2c313c"
selected_foreground = "$fg"
inactive = "$muted"
graph_text = "$fg"
misc = "$accent"

[stats]
clock = "$accent"
power = "$yellow"
fan = "$fg"
bandwidth = "$accent"

[borders]
cpu = "$muted"
gpu = "$accent"
memory = "$muted"
npu = "#c678dd"
processes = "$muted"

[gradients.temperature]
stops = [
  { at = 0.0, color = "$green" },
  { at = 0.65, color = "$yellow" },
  { at = 1.0, color = "$red" },
]

[gradients.cpu]
stops = [
  { at = 0.0, color = "$green" },
  { at = 1.0, color = "$red" },
]

[gradients.gpu]
stops = [
  { at = 0.0, color = "#56b6c2" },
  { at = 0.5, color = "$yellow" },
  { at = 1.0, color = "$red" },
]

[gradients.memory]
stops = [{ at = 0.0, color = "$accent" }]

[gradients.npu]
stops = [
  { at = 0.0, color = "#c678dd" },
  { at = 1.0, color = "$red" },
]

[gradients.process]
stops = [
  { at = 0.0, color = "$green" },
  { at = 1.0, color = "$red" },
]
```

### Colors

A color value may be:

- `#RRGGBB` truecolor;
- `#BW` two-digit grayscale;
- `R G B` decimal components;
- `$name`, referring to a literal in `[palette]`;
- `default`, for the terminal default color. This is most useful for
  `ui.background` transparency.

Palette values themselves must be color literals, not references.

### Gradient stops

The supported gradient names are `temperature`, `cpu`, `gpu`, `memory`, `npu`,
and `process`. Each gradient must have at least one stop. Stop positions must be
strictly increasing and between `0.0` and `1.0` inclusive. One stop produces a
flat color; two or more stops are linearly interpolated in RGB space.

## Bundled theme provenance

The bundled collection consists of native ports of themes distributed with
btop. Source links, retained author notices, and modification information are
recorded in [`THIRD_PARTY.md`](../THIRD_PARTY.md). Theme and palette names are
used descriptively and do not imply endorsement.
