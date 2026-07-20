#!/usr/bin/env python3
"""Capture amdtop in a pseudo-terminal and render docs/screenshot.png."""

from __future__ import annotations

import argparse
import codecs
import copy
import fcntl
import json
import math
import os
import pty
import select
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
from pathlib import Path
from typing import Any, Sequence

REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT_PATH = Path(__file__).resolve()

CPU_LOAD_PATTERNS = (
    (0.15, 0.35, 0.65, 0.90, 0.60, 0.25, 0.80, 0.45),
    (0.80, 0.55, 0.25, 0.10, 0.40, 0.75, 0.30, 0.65),
    (0.30, 0.85, 0.45, 0.70, 0.20, 0.60, 0.95, 0.35),
    (0.65, 0.20, 0.75, 0.40, 0.90, 0.30, 0.50, 0.10),
)

ANSI_COLORS = {
    "black": "000000",
    "red": "800000",
    "green": "008000",
    "brown": "808000",
    "blue": "000080",
    "magenta": "800080",
    "cyan": "008080",
    "white": "c0c0c0",
    "brightblack": "808080",
    "brightred": "ff0000",
    "brightgreen": "00ff00",
    "brightyellow": "ffff00",
    "brightblue": "0000ff",
    "brightmagenta": "ff00ff",
    "brightcyan": "00ffff",
    "brightwhite": "ffffff",
}

XTERM_BASE_COLORS = (
    "000000",
    "800000",
    "008000",
    "808000",
    "000080",
    "800080",
    "008080",
    "c0c0c0",
    "808080",
    "ff0000",
    "00ff00",
    "ffff00",
    "0000ff",
    "ff00ff",
    "00ffff",
    "ffffff",
)


def hex_rgb(value: str) -> tuple[int, int, int]:
    value = value.removeprefix("#")
    if len(value) != 6:
        raise ValueError(f"expected a six-digit RGB color, got {value!r}")
    try:
        return tuple(int(value[index : index + 2], 16) for index in (0, 2, 4))  # type: ignore[return-value]
    except ValueError as error:
        raise ValueError(f"invalid RGB color {value!r}") from error


def xterm_rgb(index: int) -> tuple[int, int, int]:
    if not 0 <= index <= 255:
        raise ValueError(f"xterm color index must be between 0 and 255, got {index}")
    if index < 16:
        return hex_rgb(XTERM_BASE_COLORS[index])
    if index < 232:
        offset = index - 16
        red, offset = divmod(offset, 36)
        green, blue = divmod(offset, 6)
        levels = (0, 95, 135, 175, 215, 255)
        return levels[red], levels[green], levels[blue]
    gray = 8 + (index - 232) * 10
    return gray, gray, gray


def color_to_rgb(value: Any, default: str) -> tuple[int, int, int]:
    fallback = hex_rgb(default)
    if value in (None, "default"):
        return fallback
    if isinstance(value, int):
        try:
            return xterm_rgb(value)
        except ValueError:
            return fallback
    if not isinstance(value, str):
        return fallback

    normalized = ANSI_COLORS.get(value.lower(), value).removeprefix("#")
    if normalized.isdecimal() and len(normalized) <= 3:
        try:
            return xterm_rgb(int(normalized))
        except ValueError:
            return fallback
    try:
        return hex_rgb(normalized)
    except ValueError:
        return fallback


def choose_auto_cores(allowed: Sequence[int], count: int) -> list[int]:
    if count <= 0 or not allowed:
        return []
    ordered = sorted(set(allowed))
    if len(ordered) <= count:
        return ordered
    if count == 1:
        return [ordered[0]]

    positions = [
        round(index * (len(ordered) - 1) / (count - 1)) for index in range(count)
    ]
    return [ordered[position] for position in dict.fromkeys(positions)]


def parse_cpu_cores(spec: str, allowed: Sequence[int], auto_count: int) -> list[int]:
    normalized = spec.strip().lower()
    if normalized in ("none", "off", ""):
        return []
    if normalized == "auto":
        return choose_auto_cores(allowed, auto_count)

    try:
        requested = list(dict.fromkeys(int(part.strip()) for part in spec.split(",")))
    except ValueError as error:
        raise ValueError(
            "CPU cores must be 'auto', 'none', or a comma-separated integer list"
        ) from error

    allowed_set = set(allowed)
    unavailable = [core for core in requested if core not in allowed_set]
    if unavailable:
        raise ValueError(
            f"CPU cores are outside this process's affinity mask: {unavailable}"
        )
    return requested


def canvas_size(
    columns: int, rows: int, cell_width: int, cell_height: int, margin: int
) -> tuple[int, int]:
    return columns * cell_width + margin * 2, rows * cell_height + margin * 2


def cpu_load_worker(cpu: int, duration: float, phase: int) -> None:
    os.sched_setaffinity(0, {cpu})
    pattern = CPU_LOAD_PATTERNS[phase % len(CPU_LOAD_PATTERNS)]
    window = 0.5
    deadline = time.monotonic() + duration
    index = 0
    value = 0.123456789

    while time.monotonic() < deadline:
        start = time.monotonic()
        busy_until = start + window * pattern[index % len(pattern)]
        while time.monotonic() < busy_until:
            value = math.sin(value) ** 2 + 0.000001
        remaining = start + window - time.monotonic()
        if remaining > 0:
            time.sleep(remaining)
        index += 1


def resolve_path(path: Path) -> Path:
    path = path.expanduser()
    return path if path.is_absolute() else REPO_ROOT / path


def resolve_font(spec: str, style: str | None = None) -> Path:
    candidate = Path(spec).expanduser()
    if candidate.is_file():
        return candidate.resolve()
    if candidate.is_absolute() or "/" in spec:
        raise FileNotFoundError(f"font file does not exist: {candidate}")

    fc_match = shutil.which("fc-match")
    if fc_match is None:
        raise FileNotFoundError(
            f"cannot resolve font family {spec!r}: install Fontconfig or pass an explicit font path"
        )
    query = f"{spec}:style={style}" if style else spec
    result = subprocess.run(
        [fc_match, "-f", "%{file}\n", query],
        check=True,
        capture_output=True,
        text=True,
    )
    matched = (
        Path(result.stdout.splitlines()[0]) if result.stdout.splitlines() else Path()
    )
    if not matched.is_file():
        raise FileNotFoundError(f"Fontconfig did not resolve font {query!r}")
    return matched.resolve()


def load_render_dependencies() -> tuple[Any, Any, Any, Any]:
    try:
        import pyte
        from PIL import Image, ImageDraw, ImageFont
    except ModuleNotFoundError as error:
        requirements = REPO_ROOT / "scripts/requirements-screenshot.txt"
        raise SystemExit(
            f"missing Python module {error.name!r}; install screenshot dependencies with:\n"
            f"  {sys.executable} -m pip install -r {requirements}"
        ) from error
    return pyte, Image, ImageDraw, ImageFont


def start_cpu_loaders(
    cores: Sequence[int], duration: float
) -> list[subprocess.Popen[bytes]]:
    loaders = [
        subprocess.Popen(
            [
                sys.executable,
                str(SCRIPT_PATH),
                "--_cpu-worker",
                str(core),
                str(duration),
                str(index),
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        for index, core in enumerate(cores)
    ]
    time.sleep(0.1)
    for loader in loaders:
        if loader.poll() is not None:
            error = (
                loader.stderr.read().decode("utf-8", "replace") if loader.stderr else ""
            )
            stop_cpu_loaders(loaders)
            raise RuntimeError(f"CPU load worker exited during setup: {error.strip()}")
    return loaders


def stop_cpu_loaders(loaders: Sequence[subprocess.Popen[bytes]]) -> None:
    for loader in loaders:
        if loader.poll() is None:
            loader.terminate()
    for loader in loaders:
        try:
            loader.wait(timeout=2)
        except subprocess.TimeoutExpired:
            loader.kill()
            loader.wait()
        if loader.stderr:
            loader.stderr.close()


def wait_for_child(pid: int, timeout: float) -> int | None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            completed_pid, status = os.waitpid(pid, os.WNOHANG)
        except ChildProcessError:
            return None
        if completed_pid == pid:
            return status
        time.sleep(0.05)
    return None


def stop_terminal_child(pid: int, master: int) -> int | None:
    try:
        os.write(master, b"q")
    except OSError:
        pass

    status = wait_for_child(pid, 3)
    if status is None:
        try:
            os.kill(pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        status = wait_for_child(pid, 2)
    if status is None:
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        status = wait_for_child(pid, 2)
    try:
        os.close(master)
    except OSError:
        pass
    return status


def copy_screen(screen: Any, columns: int, rows: int) -> list[list[Any]]:
    return [
        [copy.copy(screen.buffer[y][x]) for x in range(columns)] for y in range(rows)
    ]


def snapshot_text(snapshot: Sequence[Sequence[Any]]) -> str:
    return "\n".join("".join(cell.data for cell in row).rstrip() for row in snapshot)


def capture_terminal(
    binary: Path, config_home: Path, args: argparse.Namespace, pyte: Any
) -> tuple[list[list[Any]], float | None]:
    pid, master = pty.fork()
    if pid == 0:
        fcntl.ioctl(
            0, termios.TIOCSWINSZ, struct.pack("HHHH", args.rows, args.columns, 0, 0)
        )
        environment = os.environ.copy()
        environment["TERM"] = "xterm-256color"
        environment["COLORTERM"] = "truecolor"
        environment["XDG_CONFIG_HOME"] = str(config_home)
        os.execve(str(binary), [binary.name], environment)

    screen = pyte.Screen(args.columns, args.rows)
    stream = pyte.Stream(screen)
    decoder = codecs.getincrementaldecoder("utf-8")("replace")
    os.set_blocking(master, False)
    started = time.monotonic()
    deadline = started + args.capture_seconds
    latest_data = started
    candidate: list[list[Any]] | None = None
    candidate_elapsed: float | None = None
    child_status: int | None = None

    try:
        while time.monotonic() < deadline:
            ready, _, _ = select.select([master], [], [], 0.05)
            if ready:
                try:
                    data = os.read(master, 65536)
                except BlockingIOError:
                    continue
                except OSError:
                    break
                if not data:
                    break
                stream.feed(decoder.decode(data))
                latest_data = time.monotonic()
                continue

            elapsed = time.monotonic() - started
            if (
                args.prefer_text
                and elapsed >= args.min_candidate_seconds
                and time.monotonic() - latest_data >= 0.03
                and args.prefer_text in "\n".join(screen.display)
            ):
                candidate = copy_screen(screen, args.columns, args.rows)
                candidate_elapsed = elapsed
    finally:
        child_status = stop_terminal_child(pid, master)

    if child_status is not None and os.waitstatus_to_exitcode(child_status) != 0:
        raise RuntimeError(
            f"amdtop exited with status {os.waitstatus_to_exitcode(child_status)}"
        )

    if args.require_text and candidate is None:
        raise RuntimeError(
            f"required text {args.prefer_text!r} did not appear after "
            f"{args.min_candidate_seconds:g}s during the capture"
        )

    snapshot = candidate or copy_screen(screen, args.columns, args.rows)
    text = snapshot_text(snapshot)
    for expected in ("amdtop", "PROCESSES"):
        if expected not in text:
            raise RuntimeError(
                f"captured terminal does not contain expected text {expected!r}"
            )
    return snapshot, candidate_elapsed


def render_snapshot(
    snapshot: Sequence[Sequence[Any]],
    output: Path,
    normal_font_path: Path,
    bold_font_path: Path,
    args: argparse.Namespace,
    Image: Any,
    ImageDraw: Any,
    ImageFont: Any,
) -> tuple[int, int]:
    default_background = color_to_rgb(args.default_bg, args.default_bg)
    width, height = canvas_size(
        args.columns, args.rows, args.cell_width, args.cell_height, args.margin
    )
    image = Image.new("RGB", (width, height), default_background)
    draw = ImageDraw.Draw(image)
    normal_font = ImageFont.truetype(str(normal_font_path), args.font_size)
    bold_font = ImageFont.truetype(str(bold_font_path), args.font_size)

    for row_index, row in enumerate(snapshot):
        for column_index, cell in enumerate(row):
            left = args.margin + column_index * args.cell_width
            top = args.margin + row_index * args.cell_height
            foreground = color_to_rgb(cell.fg, args.default_fg)
            background = color_to_rgb(cell.bg, args.default_bg)
            if cell.reverse:
                foreground, background = background, foreground
            if background != default_background:
                draw.rectangle(
                    (left, top, left + args.cell_width - 1, top + args.cell_height - 1),
                    fill=background,
                )
            if not cell.data or cell.data == " ":
                continue
            font = bold_font if cell.bold else normal_font
            draw.text(
                (left, top + args.text_offset_y), cell.data, font=font, fill=foreground
            )
            if getattr(cell, "underscore", False):
                underline_y = top + args.cell_height - 2
                draw.line(
                    (left, underline_y, left + args.cell_width - 1, underline_y),
                    fill=foreground,
                )

    output.parent.mkdir(parents=True, exist_ok=True)
    temporary_output = output.with_name(f".{output.name}.{os.getpid()}.tmp")
    try:
        image.save(temporary_output, format="PNG", optimize=True)
        temporary_output.chmod(0o644)
        os.replace(temporary_output, output)
    finally:
        temporary_output.unlink(missing_ok=True)
    return width, height


def write_capture_state(config_home: Path, theme: str, block_style: int) -> None:
    state_path = config_home / "amdtop/state.json"
    state_path.parent.mkdir(parents=True, exist_ok=True)
    state_path.write_text(
        json.dumps(
            {
                "cpu": False,
                "gpu": False,
                "npu": False,
                "processes": False,
                "theme": theme,
                "block_style": block_style,
            }
        )
        + "\n",
        encoding="utf-8",
    )


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return parsed


def nonnegative_int(value: str) -> int:
    parsed = int(value)
    if parsed < 0:
        raise argparse.ArgumentTypeError("must not be negative")
    return parsed


def positive_float(value: str) -> float:
    parsed = float(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return parsed


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Capture amdtop in a pseudo-terminal and render a configurable PNG screenshot.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("--output", type=Path, default=Path("docs/screenshot.png"))
    parser.add_argument(
        "--binary",
        type=Path,
        help="executable to capture; defaults to TARGET_DIR/debug/amdtop",
    )
    parser.add_argument(
        "--target-dir",
        type=Path,
        default=Path("target/screenshot"),
        help="isolated Cargo target directory used for the default binary",
    )
    parser.add_argument(
        "--no-build",
        action="store_true",
        help="use the existing binary without running cargo build",
    )
    parser.add_argument(
        "--columns", type=positive_int, default=188, help="terminal character columns"
    )
    parser.add_argument(
        "--rows", type=positive_int, default=46, help="terminal character rows"
    )
    parser.add_argument("--capture-seconds", type=positive_float, default=13.0)
    parser.add_argument("--min-candidate-seconds", type=positive_float, default=8.0)
    parser.add_argument(
        "--prefer-text", help="retain the latest mature frame containing this text"
    )
    parser.add_argument(
        "--require-text",
        action="store_true",
        help="fail unless --prefer-text appears in a mature frame",
    )
    parser.add_argument(
        "--cpu-load-cores",
        default="auto",
        help="'auto', 'none', or comma-separated CPU IDs",
    )
    parser.add_argument(
        "--cpu-load-count",
        type=nonnegative_int,
        default=4,
        help="worker count when cores are auto-selected",
    )
    parser.add_argument(
        "--font",
        default="Iosevka Term Nerd Font Mono",
        help="normal font family or file",
    )
    parser.add_argument(
        "--bold-font",
        help="bold font family or file; inferred from --font when omitted",
    )
    parser.add_argument(
        "--font-size", type=positive_int, default=19, help="font size in pixels"
    )
    parser.add_argument(
        "--cell-width",
        type=positive_int,
        default=10,
        help="rendered character-cell width in pixels",
    )
    parser.add_argument(
        "--cell-height",
        type=positive_int,
        default=20,
        help="rendered character-cell height in pixels",
    )
    parser.add_argument(
        "--text-offset-y",
        type=int,
        default=-1,
        help="vertical glyph offset within each cell",
    )
    parser.add_argument(
        "--margin", type=nonnegative_int, default=4, help="canvas margin in pixels"
    )
    parser.add_argument(
        "--default-fg", default="cfc9c2", help="six-digit default foreground RGB"
    )
    parser.add_argument(
        "--default-bg", default="1a1b26", help="six-digit default background RGB"
    )
    parser.add_argument(
        "--theme",
        default="tokyo-night",
        help="amdtop theme stored in the isolated capture state",
    )
    parser.add_argument(
        "--block-style",
        type=nonnegative_int,
        default=0,
        help="amdtop gauge block-style index",
    )
    parser.add_argument(
        "--dump-text",
        type=Path,
        help="also write the selected terminal frame as UTF-8 text",
    )
    parser.add_argument(
        "--print-screen", action="store_true", help="print the selected terminal frame"
    )
    parser.add_argument(
        "--_cpu-worker",
        nargs=3,
        metavar=("CPU", "SECONDS", "PHASE"),
        help=argparse.SUPPRESS,
    )
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()

    if args._cpu_worker:
        cpu_load_worker(
            int(args._cpu_worker[0]),
            float(args._cpu_worker[1]),
            int(args._cpu_worker[2]),
        )
        return 0

    if args.require_text and not args.prefer_text:
        parser.error("--require-text requires --prefer-text")
    if args.min_candidate_seconds > args.capture_seconds:
        parser.error("--min-candidate-seconds cannot exceed --capture-seconds")
    try:
        hex_rgb(args.default_fg)
        hex_rgb(args.default_bg)
    except ValueError as error:
        parser.error(str(error))

    pyte, Image, ImageDraw, ImageFont = load_render_dependencies()
    target_dir = resolve_path(args.target_dir)
    binary = resolve_path(args.binary) if args.binary else target_dir / "debug/amdtop"
    output = resolve_path(args.output)
    dump_text = resolve_path(args.dump_text) if args.dump_text else None

    if not args.no_build:
        # A package-only clean in a dedicated target directory prevents another
        # Cargo profile or an old binary from leaking into the documentation.
        subprocess.run(
            ["cargo", "clean", "--target-dir", str(target_dir), "--package", "amdtop"],
            cwd=REPO_ROOT,
            check=True,
        )
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
        parser.error(f"amdtop binary is not executable: {binary}")

    normal_font_path = resolve_font(args.font)
    if args.bold_font:
        bold_font_path = resolve_font(args.bold_font)
    elif Path(args.font).expanduser().is_file():
        bold_font_path = normal_font_path
    else:
        bold_font_path = resolve_font(args.font, "Bold")

    allowed_cores = sorted(os.sched_getaffinity(0))
    try:
        load_cores = parse_cpu_cores(
            args.cpu_load_cores, allowed_cores, args.cpu_load_count
        )
    except ValueError as error:
        parser.error(str(error))

    loaders: list[subprocess.Popen[bytes]] = []
    with tempfile.TemporaryDirectory(
        prefix="amdtop-screenshot-"
    ) as temporary_directory:
        config_home = Path(temporary_directory)
        write_capture_state(config_home, args.theme, args.block_style)
        try:
            loaders = start_cpu_loaders(load_cores, args.capture_seconds + 3)
            snapshot, candidate_elapsed = capture_terminal(
                binary, config_home, args, pyte
            )
        finally:
            stop_cpu_loaders(loaders)

    text = snapshot_text(snapshot)
    if dump_text:
        dump_text.parent.mkdir(parents=True, exist_ok=True)
        dump_text.write_text(text + "\n", encoding="utf-8")
    if args.print_screen:
        print(text)

    width, height = render_snapshot(
        snapshot,
        output,
        normal_font_path,
        bold_font_path,
        args,
        Image,
        ImageDraw,
        ImageFont,
    )
    selection = (
        f"preferred frame at {candidate_elapsed:.2f}s"
        if candidate_elapsed is not None
        else f"final frame at {args.capture_seconds:g}s"
    )
    if args.prefer_text and candidate_elapsed is None:
        print(
            f"warning: preferred text {args.prefer_text!r} was not captured; using final frame",
            file=sys.stderr,
        )
    print(
        f"captured {args.columns}x{args.rows} terminal using CPU cores {load_cores or 'none'} ({selection})"
    )
    print(f"font: {normal_font_path}")
    print(f"bold font: {bold_font_path}")
    print(f"wrote {output} ({width}x{height}, {output.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        print("error: screenshot capture interrupted", file=sys.stderr)
        raise SystemExit(130) from None
    except (FileNotFoundError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1) from None
