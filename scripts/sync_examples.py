"""Synchronize repository example cartridges into Glaux's data directory.

Usage examples:
    mise run sync-examples
    mise run sync-examples -- signal-lost
    mise run sync-examples -- --dry-run
    mise run sync-examples -- --destination /path/to/cartridges
"""

from __future__ import annotations

import argparse
import ctypes
import hashlib
import os
import shutil
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parent.parent
EXAMPLES_DIR = REPOSITORY_ROOT / "examples"
ROOT_ASSETS = frozenset({"sprites.bin", "objattr.bin", "obp.bin", "palette.bin"})


class SyncError(Exception):
    """An expected discovery, path, or filesystem failure."""


@dataclass(frozen=True)
class Cartridge:
    """A discovered example cartridge and the runtime files it supplies."""

    name: str
    root: Path
    files: tuple[Path, ...]


@dataclass(frozen=True)
class PlannedFile:
    """One source file that needs to be copied to its destination path."""

    source: Path
    destination: Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "examples",
        nargs="*",
        metavar="EXAMPLE",
        help="example directory name to synchronize (all examples by default)",
    )
    parser.add_argument(
        "--destination",
        type=Path,
        help="cartridge root to use instead of Glaux's platform data directory",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="show what would change without writing anything",
    )
    return parser.parse_args()


def windows_roaming_app_data() -> Path:
    from ctypes import wintypes

    class Guid(ctypes.Structure):
        _fields_ = [
            ("data1", wintypes.DWORD),
            ("data2", wintypes.WORD),
            ("data3", wintypes.WORD),
            ("data4", ctypes.c_ubyte * 8),
        ]

    folder_id = Guid(
        0x3EB685DB,
        0x65F9,
        0x4CF6,
        (ctypes.c_ubyte * 8)(0xA0, 0x3A, 0xE3, 0xEF, 0x65, 0x72, 0x9F, 0x3D),
    )
    path = ctypes.c_wchar_p()
    shell = ctypes.windll.shell32
    result = shell.SHGetKnownFolderPath(ctypes.byref(folder_id), 0, None, ctypes.byref(path))
    if result != 0:
        raise SyncError(f"cannot locate Windows roaming AppData (error 0x{result:08x})")
    try:
        return Path(path.value)
    finally:
        ctypes.windll.ole32.CoTaskMemFree(path)


def default_destination() -> Path:
    """Return the cartridge root corresponding to Glaux's Rust ProjectDirs."""
    if sys.platform == "win32":
        data_dir = windows_roaming_app_data() / "cuervolu" / "Glaux" / "data"
    elif sys.platform == "darwin":
        data_dir = Path.home() / "Library" / "Application Support" / "dev.cuervolu.Glaux"
    elif sys.platform.startswith(("linux", "freebsd", "openbsd", "netbsd")):
        configured = os.environ.get("XDG_DATA_HOME")
        xdg_data = Path(configured) if configured else None
        if xdg_data is None or not xdg_data.is_absolute():
            xdg_data = Path.home() / ".local" / "share"
        data_dir = xdg_data / "glaux"
    else:
        raise SyncError(
            f"cannot determine Glaux's data directory on {sys.platform!r}; "
            "pass --destination"
        )
    return data_dir / "cartridges"


def runtime_files(root: Path) -> tuple[Path, ...]:
    files = [root / "cartridge.ron"]

    def raise_walk_error(error: OSError) -> None:
        raise error

    try:
        for current, directories, names in os.walk(
            root, followlinks=False, onerror=raise_walk_error
        ):
            current_path = Path(current)
            for name in directories:
                path = current_path / name
                if path.is_symlink():
                    raise SyncError(f"source contains a symlink: {path}")
            for name in names:
                path = current_path / name
                if path.is_symlink():
                    raise SyncError(f"source contains a symlink: {path}")
                relative = path.relative_to(root)
                if relative == Path("cartridge.ron"):
                    continue
                is_runtime = path.suffix == ".luau" or (
                    relative.parent == Path() and name in ROOT_ASSETS
                )
                if is_runtime and not path.is_file():
                    raise SyncError(f"source path is not a regular file: {path}")
                if is_runtime:
                    files.append(path)
    except OSError as error:
        raise SyncError(f"cannot inspect {root}: {error}") from error
    return tuple(sorted(files, key=lambda path: path.relative_to(root).as_posix()))


def discover_cartridges() -> dict[str, Cartridge]:
    try:
        entries = sorted(EXAMPLES_DIR.iterdir(), key=lambda path: path.name.casefold())
    except OSError as error:
        raise SyncError(f"cannot read {EXAMPLES_DIR}: {error}") from error

    cartridges: dict[str, Cartridge] = {}
    names_by_case: dict[str, str] = {}
    for entry in entries:
        if entry.is_symlink() or not entry.is_dir():
            continue
        manifest = entry / "cartridge.ron"
        if manifest.is_symlink() or not manifest.is_file():
            continue
        folded = entry.name.casefold()
        if folded in names_by_case:
            other = names_by_case[folded]
            raise SyncError(
                f"example names conflict across platforms: {other!r} and {entry.name!r}"
            )
        names_by_case[folded] = entry.name
        cartridges[entry.name] = Cartridge(entry.name, entry, runtime_files(entry))
    return cartridges


def select_cartridges(
    available: dict[str, Cartridge], requested: list[str]
) -> list[Cartridge]:
    if not requested:
        return list(available.values())

    unknown = sorted(set(requested) - available.keys())
    if unknown:
        choices = ", ".join(available) or "none"
        raise SyncError(
            f"unknown example(s): {', '.join(unknown)}; available examples: {choices}"
        )
    return [available[name] for name in dict.fromkeys(requested)]


def file_digest(path: Path) -> bytes:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.digest()


def inspect_destination_path(cartridge_root: Path, relative: Path) -> Path:
    current = cartridge_root
    for component in relative.parts[:-1]:
        current /= component
        if current.is_symlink():
            raise SyncError(f"destination contains a symlink: {current}")
        if current.exists() and not current.is_dir():
            raise SyncError(f"destination path is not a directory: {current}")

    target = cartridge_root / relative
    if target.is_symlink():
        raise SyncError(f"destination file is a symlink: {target}")
    if target.exists() and not target.is_file():
        raise SyncError(f"destination path is not a regular file: {target}")
    return target


def plan_cartridge(cartridge: Cartridge, destination: Path) -> tuple[bool, list[PlannedFile]]:
    target_root = destination / cartridge.name
    if target_root.is_symlink():
        raise SyncError(f"destination cartridge is a symlink: {target_root}")
    existed = target_root.exists()
    if existed and not target_root.is_dir():
        raise SyncError(f"destination cartridge is not a directory: {target_root}")

    planned = []
    for source in cartridge.files:
        relative = source.relative_to(cartridge.root)
        target = inspect_destination_path(target_root, relative)
        try:
            identical = target.exists() and file_digest(source) == file_digest(target)
        except OSError as error:
            raise SyncError(f"cannot compare {relative}: {error}") from error
        if not identical:
            planned.append(PlannedFile(source, target))
    return existed, planned


def copy_file(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            dir=destination.parent, prefix=f".{destination.name}.", delete=False
        ) as file:
            temporary = Path(file.name)
        shutil.copy2(source, temporary, follow_symlinks=False)
        os.replace(temporary, destination)
        temporary = None
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def sync_cartridge(cartridge: Cartridge, destination: Path, dry_run: bool) -> str:
    existed, planned = plan_cartridge(cartridge, destination)
    if not planned:
        return "unchanged"
    if not dry_run:
        for operation in planned:
            try:
                copy_file(operation.source, operation.destination)
            except OSError as error:
                relative = operation.source.relative_to(cartridge.root)
                raise SyncError(f"cannot copy {relative}: {error}") from error
    return "updated" if existed else "installed"


def main() -> int:
    args = parse_args()
    try:
        available = discover_cartridges()
        cartridges = select_cartridges(available, args.examples)
        requested_destination = args.destination or default_destination()
        destination = requested_destination.expanduser().resolve(strict=False)
        if destination.exists() and not destination.is_dir():
            raise SyncError(f"destination is not a directory: {destination}")
    except (OSError, SyncError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2

    counts = {status: 0 for status in ("installed", "updated", "unchanged", "skipped")}
    for cartridge in cartridges:
        try:
            status = sync_cartridge(cartridge, destination, args.dry_run)
            counts[status] += 1
            prefix = "would be " if args.dry_run and status != "unchanged" else ""
            print(f"{cartridge.name}: {prefix}{status}")
        except SyncError as error:
            counts["skipped"] += 1
            print(f"{cartridge.name}: skipped ({error})", file=sys.stderr)

    summary = ", ".join(f"{counts[status]} {status}" for status in counts)
    heading = "Summary (dry run)" if args.dry_run else "Summary"
    print(f"{heading}: {summary}")
    return 1 if counts["skipped"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
