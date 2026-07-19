# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.1] - 2026-07-19

### Added

- Added `--help` and `--version` command-line options.
- Added unit and integration coverage for configuration, telemetry parsers,
  sampling, gauges, history graphs, themes, layout helpers, and CLI behavior.
- Added crates.io package metadata and documented the amdtop state path.

### Changed

- Declared Rust 1.88 as the minimum supported toolchain.
- Moved the hardware smoke utility to a Cargo example so installation produces
  only the `amdtop` executable.
- Split TUI rendering into focused CPU, GPU, NPU, and process modules.
- Simplified GPU memory selection, cached parsed theme colors, and used a ring
  buffer for telemetry history.
- Applied rustfmt and removed dead code and Clippy warnings throughout.

### Fixed

- Honor `XDG_CONFIG_HOME` for persisted UI state and write state atomically.
- Use real elapsed time for telemetry samples instead of resampling on every
  keypress.
- Restore terminal state reliably after setup errors, runtime errors, or panics.
- Correct Linux CPU accounting for `guest_nice` time and reject malformed
  `/proc/stat` samples.
- Correct xterm indexed-color conversion and strict decimal theme parsing.
- Clamp memory percentages, handle zero-sized GPU memory pools, and ignore zero
  GPU power caps.
- Use conventional disclosure indicators for collapsed and expanded sections.
- Refresh the README screenshot and all release documentation for amdtop.

## [0.2.0] - 2026-07-19

### Changed

- Renamed the project, Cargo package, executable, configuration directory, and
  UI title to `amdtop`.
- Reframed amdtop as an independent application inspired by the modern TUI
  visual style of nvitop and btop.
- Switched AMD GPU telemetry to the published `libamdgpu_top` crate from
  crates.io instead of an unpinned Git dependency.
- Updated installation, usage, credits, and repository links for the new name.

## [0.1.0] - 2026-06-19

### Added

- Initial btop/nvitop-inspired terminal monitor for AMD systems.
- CPU utilization, per-core history, memory, swap, load, temperature, and power
  monitoring.
- Multi-GPU utilization, memory, sensor, clock, and process monitoring.
- XDNA NPU detection and optional DRM fdinfo telemetry.
- Collapsible sections, persistent UI state, btop theme support, and selectable
  gauge styles.

[Unreleased]: https://github.com/lhl/amdtop/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/lhl/amdtop/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/lhl/amdtop/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/lhl/amdtop/releases/tag/v0.1.0
