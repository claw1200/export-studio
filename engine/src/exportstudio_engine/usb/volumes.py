import plistlib
import subprocess
from pathlib import Path

from .paths import ROOT_NAMES, has_library


def _ejectable(path: Path) -> bool:
    try:
        raw = subprocess.check_output(["diskutil", "info", "-plist", str(path)], timeout=3, stderr=subprocess.DEVNULL)
        info = plistlib.loads(raw)
    except (OSError, subprocess.SubprocessError, plistlib.InvalidFileException):
        return False
    return bool(info.get("RemovableMedia") or info.get("Ejectable")) and not info.get("Internal")


def _root_name(volume: Path) -> str | None:
    for name in ROOT_NAMES:
        rekordbox = volume / name / "rekordbox"
        if rekordbox.is_dir():
            return name
    return None


def list_volumes(root: Path | None = None) -> list[dict]:
    """Mounted volumes. The boot volume is skipped when scanning /Volumes."""
    real = root is None
    base = Path("/Volumes") if root is None else root
    if not base.is_dir():
        return []
    boot = None
    if real:
        try:
            boot = Path("/").stat().st_dev
        except OSError:
            boot = None
    volumes = []
    for path in sorted((p for p in base.iterdir() if p.is_dir()), key=lambda p: p.name.casefold()):
        try:
            dev = path.stat().st_dev
        except OSError:
            continue
        library = has_library(path)
        if real and (dev == boot or (not library and not _ejectable(path))):
            continue
        volumes.append(
            {
                "id": str(path),
                "name": path.name,
                "path": str(path),
                "hasLibrary": library,
                "rootName": _root_name(path),
            }
        )
    return volumes
