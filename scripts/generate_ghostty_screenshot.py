#!/usr/bin/env python3
"""Capture amdtop through the real Ghostty+niri rendering path."""

from __future__ import annotations

import argparse
import fcntl
import json
import os
import signal
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO_ROOT))

from scripts.generate_screenshot import (  # noqa: E402
    choose_auto_cores,
    parse_cpu_cores,
    start_cpu_loaders,
    stop_cpu_loaders,
)

DEFAULT_OUTPUT = REPO_ROOT / "docs/screenshot.png"
DEFAULT_TARGET_DIR = REPO_ROOT / "target/screenshot"


def resize_for_grid(
    width: int,
    height: int,
    columns: int,
    rows: int,
    target_columns: int,
    target_rows: int,
) -> tuple[int, int]:
    if min(width, height, columns, rows, target_columns, target_rows) <= 0:
        raise ValueError("window and grid dimensions must be positive")
    return (
        (width * target_columns + columns // 2) // columns,
        (height * target_rows + rows // 2) // rows,
    )


def expanded_capture_state(state: dict[str, Any], theme: str) -> bool:
    return (
        state.get("theme") == theme
        and all(state.get(section) is False for section in ("cpu", "gpu", "npu", "processes"))
    )


def select_window(windows: list[dict[str, Any]], title: str) -> dict[str, Any] | None:
    return next((window for window in windows if window.get("title") == title), None)


def command_path(name: str) -> str:
    from shutil import which

    path = which(name)
    if path is None:
        raise RuntimeError(f"required command not found: {name}")
    return path


def niri_windows() -> list[dict[str, Any]]:
    result = subprocess.run(
        ["niri", "msg", "--json", "windows"],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    return json.loads(result.stdout)


def niri_action(*args: str) -> None:
    subprocess.run(["niri", "msg", "action", *args], check=True, stdout=subprocess.DEVNULL)


def wait_for_window(title: str, timeout: float = 10.0) -> dict[str, Any]:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if window := select_window(niri_windows(), title):
            return window
        time.sleep(0.1)
    raise RuntimeError(f"Ghostty window {title!r} did not appear")


def descendants(pid: int) -> list[int]:
    found: list[int] = []
    pending = [pid]
    while pending:
        parent = pending.pop()
        children: set[int] = set()
        for children_path in Path(f"/proc/{parent}/task").glob("*/children"):
            try:
                children.update(int(value) for value in children_path.read_text().split())
            except (FileNotFoundError, PermissionError, ValueError):
                continue
        new_children = [child for child in children if child not in found]
        found.extend(new_children)
        pending.extend(new_children)
    return found


def wait_for_amdtop(ghostty_pid: int, binary: Path, timeout: float = 10.0) -> tuple[int, Path]:
    expected = str(binary)
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for pid in descendants(ghostty_pid):
            try:
                command = Path(f"/proc/{pid}/cmdline").read_bytes().replace(b"\0", b" ").decode()
                tty = Path(os.readlink(f"/proc/{pid}/fd/0"))
            except (FileNotFoundError, PermissionError, OSError, UnicodeDecodeError):
                continue
            if expected in command and str(tty).startswith("/dev/pts/"):
                return pid, tty
        time.sleep(0.1)
    raise RuntimeError("amdtop did not start under Ghostty")


def tty_grid(tty: Path) -> tuple[int, int]:
    fd = os.open(tty, os.O_RDONLY | os.O_NOCTTY)
    try:
        rows, columns, _, _ = struct.unpack("HHHH", fcntl.ioctl(fd, 0x5413, b"\0" * 8))
    finally:
        os.close(fd)
    return columns, rows


def wait_for_exit(process: subprocess.Popen[bytes], timeout: float) -> bool:
    try:
        process.wait(timeout=timeout)
        return True
    except subprocess.TimeoutExpired:
        return False


def stop_process_group(process: subprocess.Popen[bytes] | None) -> None:
    if process is None or process.poll() is not None:
        return
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    if not wait_for_exit(process, 3):
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait(timeout=3)


def png_dimensions(path: Path) -> tuple[int, int]:
    header = path.read_bytes()[:24]
    if len(header) != 24 or header[:8] != b"\x89PNG\r\n\x1a\n":
        raise RuntimeError(f"niri did not produce a valid PNG: {path}")
    return struct.unpack(">II", header[16:24])


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--columns", type=int, default=120)
    parser.add_argument("--rows", type=int, default=48)
    parser.add_argument("--font-family", default="JetBrainsMono Nerd Font Mono")
    parser.add_argument("--font-size", type=float, default=14.0)
    parser.add_argument("--theme", default="tokyo-night")
    parser.add_argument("--block-style", type=int, default=0)
    parser.add_argument("--capture-seconds", type=float, default=13.0)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--target-dir", type=Path, default=DEFAULT_TARGET_DIR)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--cpu-load-cores", default="auto")
    parser.add_argument("--cpu-load-workers", type=int, default=4)
    parser.add_argument("--gpu-load", action=argparse.BooleanOptionalAction, default=True)
    parser.add_argument("--gpu-load-env", default="therock")
    parser.add_argument("--gpu-matrix-size", type=int, default=4096)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    if min(args.columns, args.rows, args.font_size, args.capture_seconds) <= 0:
        raise SystemExit("columns, rows, font-size, and capture-seconds must be positive")

    for command in ("cargo", "ghostty", "niri"):
        command_path(command)
    if args.gpu_load:
        command_path("mamba")

    target_dir = args.target_dir.resolve()
    binary = args.binary.resolve() if args.binary else target_dir / "debug/amdtop"
    if not args.no_build:
        subprocess.run(
            [
                "cargo",
                "build",
                "--locked",
                "--bin",
                "amdtop",
                "--target-dir",
                str(target_dir),
            ],
            cwd=REPO_ROOT,
            check=True,
        )
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise SystemExit(f"amdtop binary is not executable: {binary}")

    allowed_cores = sorted(os.sched_getaffinity(0))
    load_cores = parse_cpu_cores(args.cpu_load_cores, allowed_cores, args.cpu_load_workers)
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)

    cpu_loaders: list[subprocess.Popen[bytes]] = []
    gpu_loader: subprocess.Popen[bytes] | None = None
    ghostty: subprocess.Popen[bytes] | None = None
    app_pid: int | None = None
    window_id: int | None = None

    with tempfile.TemporaryDirectory(prefix="amdtop-ghostty-") as temporary:
        temporary_path = Path(temporary)
        state_path = temporary_path / "amdtop/state.json"
        state_path.parent.mkdir(parents=True)
        state_path.write_text(
            json.dumps(
                {
                    "cpu": False,
                    "gpu": False,
                    "npu": False,
                    "processes": False,
                    "theme": args.theme,
                    "block_style": args.block_style,
                }
            )
        )
        title = f"amdtop-screenshot-{os.getpid()}"
        temporary_png = output.with_name(f".{output.name}.{os.getpid()}.tmp")
        temporary_png.unlink(missing_ok=True)
        ghostty_log = temporary_path / "ghostty.log"
        gpu_log = temporary_path / "gpu-load.log"

        try:
            cpu_loaders = start_cpu_loaders(load_cores, args.capture_seconds + 15)
            if args.gpu_load:
                gpu_log_file = gpu_log.open("wb")
                gpu_loader = subprocess.Popen(
                    [
                        "mamba",
                        "run",
                        "-n",
                        args.gpu_load_env,
                        "python",
                        str(REPO_ROOT / "scripts/gpu_load.py"),
                        "--seconds",
                        str(args.capture_seconds + 25),
                        "--phase-seconds",
                        "1.5",
                        "--levels",
                        "0.2,0.45,0.7,1.0",
                        "--matrix-size",
                        str(args.gpu_matrix_size),
                    ],
                    cwd=REPO_ROOT,
                    stdout=gpu_log_file,
                    stderr=subprocess.STDOUT,
                    start_new_session=True,
                )

            ghostty_log_file = ghostty_log.open("wb")
            ghostty = subprocess.Popen(
                [
                    "ghostty",
                    "--gtk-single-instance=false",
                    f"--title={title}",
                    f"--font-family={args.font_family}",
                    f"--font-size={args.font_size}",
                    "--window-decoration=false",
                    "--window-padding-x=0",
                    "--window-padding-y=0",
                    "--window-save-state=never",
                    "--confirm-close-surface=false",
                    "--background=1a1b26",
                    "--foreground=cfc9c2",
                    f"--window-width={args.columns}",
                    f"--window-height={args.rows}",
                    "-e",
                    "env",
                    f"XDG_CONFIG_HOME={temporary_path}",
                    str(binary),
                ],
                cwd=REPO_ROOT,
                stdout=ghostty_log_file,
                stderr=subprocess.STDOUT,
            )

            window = wait_for_window(title)
            window_id = int(window["id"])
            niri_action("toggle-window-floating", "--id", str(window_id))
            time.sleep(0.5)
            app_pid, tty = wait_for_amdtop(ghostty.pid, binary)

            for _ in range(4):
                columns, rows = tty_grid(tty)
                if (columns, rows) == (args.columns, args.rows):
                    break
                current = select_window(niri_windows(), title)
                if current is None:
                    raise RuntimeError("Ghostty window disappeared during sizing")
                width, height = current["layout"]["window_size"]
                width, height = resize_for_grid(
                    int(width), int(height), columns, rows, args.columns, args.rows
                )
                niri_action("set-window-width", "--id", str(window_id), str(width))
                niri_action("set-window-height", "--id", str(window_id), str(height))
                time.sleep(0.5)
            columns, rows = tty_grid(tty)
            if (columns, rows) != (args.columns, args.rows):
                raise RuntimeError(
                    f"Ghostty grid is {columns}x{rows}, expected {args.columns}x{args.rows}"
                )

            time.sleep(args.capture_seconds)
            state = json.loads(state_path.read_text())
            if not expanded_capture_state(state, args.theme):
                raise RuntimeError(f"capture state changed unexpectedly: {state}")

            niri_action(
                "screenshot-window",
                "--id",
                str(window_id),
                "--write-to-disk",
                "true",
                "--show-pointer",
                "false",
                "--path",
                str(temporary_png),
            )
            deadline = time.monotonic() + 3
            while time.monotonic() < deadline and not temporary_png.is_file():
                time.sleep(0.05)
            if not temporary_png.is_file():
                raise RuntimeError("niri did not finish writing the screenshot")
            width, height = png_dimensions(temporary_png)
            os.replace(temporary_png, output)

            os.kill(app_pid, signal.SIGTERM)
            app_pid = None
            if not wait_for_exit(ghostty, 5):
                raise RuntimeError("Ghostty did not exit after amdtop terminated")
            ghostty = None
            window_id = None
        finally:
            if app_pid is not None:
                try:
                    os.kill(app_pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
            if ghostty is not None and ghostty.poll() is None:
                if window_id is not None:
                    try:
                        niri_action("close-window", "--id", str(window_id))
                    except subprocess.CalledProcessError:
                        pass
                if not wait_for_exit(ghostty, 2):
                    ghostty.terminate()
                    wait_for_exit(ghostty, 2)
            stop_process_group(gpu_loader)
            stop_cpu_loaders(cpu_loaders)
            temporary_png.unlink(missing_ok=True)
            if 'gpu_log_file' in locals():
                gpu_log_file.close()
            if 'ghostty_log_file' in locals():
                ghostty_log_file.close()

    print(f"captured {args.columns}x{args.rows} Ghostty grid using CPU cores {load_cores or 'none'}")
    print(f"font: {args.font_family} {args.font_size:g} pt")
    print(f"wrote {output} ({width}x{height}, {output.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
