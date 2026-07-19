# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/lhl/amdtop/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/lhl/amdtop/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/lhl/amdtop/releases/tag/v0.1.0
