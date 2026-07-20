#!/usr/bin/env python3
"""Bounded, sweeping ROCm GPU load for amdtop screenshot captures."""

from __future__ import annotations

import argparse
import ctypes
import time
from collections.abc import Sequence

DEFAULT_LEVELS = (0.2, 0.45, 0.7, 1.0)


def duty_cycle(elapsed: float, phase_seconds: float, levels: Sequence[float]) -> float:
    """Return the repeating duty level for an elapsed monotonic duration."""
    if phase_seconds <= 0:
        raise ValueError("phase_seconds must be positive")
    if not levels:
        raise ValueError("at least one duty level is required")
    if any(level <= 0 or level > 1 for level in levels):
        raise ValueError("duty levels must be greater than 0 and at most 1")
    phase = int(max(0.0, elapsed) / phase_seconds)
    return levels[phase % len(levels)]


def set_process_name(name: str) -> None:
    """Set Linux's short process name so amdtop labels the workload clearly."""
    libc = ctypes.CDLL(None)
    pr_set_name = 15
    libc.prctl(pr_set_name, name.encode()[:15], 0, 0, 0)


def parse_levels(value: str) -> tuple[float, ...]:
    try:
        levels = tuple(float(item) for item in value.split(","))
    except ValueError as error:
        raise argparse.ArgumentTypeError("levels must be comma-separated numbers") from error
    try:
        duty_cycle(0.0, 1.0, levels)
    except ValueError as error:
        raise argparse.ArgumentTypeError(str(error)) from error
    return levels


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seconds", type=float, default=60.0)
    parser.add_argument("--phase-seconds", type=float, default=1.5)
    parser.add_argument("--levels", type=parse_levels, default=DEFAULT_LEVELS)
    parser.add_argument("--matrix-size", type=int, default=4096)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    if args.seconds <= 0 or args.phase_seconds <= 0 or args.matrix_size <= 0:
        raise SystemExit("seconds, phase-seconds, and matrix-size must be positive")

    import torch

    if not torch.cuda.is_available():
        raise SystemExit("PyTorch reports no ROCm/CUDA device")

    set_process_name("amdtop-gpu-load")
    device = torch.device("cuda")
    dtype = torch.bfloat16
    torch.manual_seed(0)
    left = torch.randn((args.matrix_size, args.matrix_size), device=device, dtype=dtype)
    right = torch.randn((args.matrix_size, args.matrix_size), device=device, dtype=dtype)
    output = torch.empty_like(left)

    for _ in range(3):
        torch.mm(left, right, out=output)
    torch.cuda.synchronize()

    started = time.monotonic()
    launches = 0
    while (elapsed := time.monotonic() - started) < args.seconds:
        duty = duty_cycle(elapsed, args.phase_seconds, args.levels)
        within_phase = elapsed % args.phase_seconds
        active_seconds = duty * args.phase_seconds
        if within_phase < active_seconds:
            torch.mm(left, right, out=output)
            torch.cuda.synchronize()
            launches += 1
        else:
            remaining = args.phase_seconds - within_phase
            time.sleep(min(0.02, remaining, args.seconds - elapsed))

    print(
        f"completed {launches} BF16 matrix multiplies over {args.seconds:.1f}s "
        f"with duty sweep {args.levels}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
