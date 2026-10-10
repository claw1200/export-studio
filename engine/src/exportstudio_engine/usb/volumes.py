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
        if real and dev == boot:
            continue
        library = has_library(path)
        ejectable = _ejectable(path) if real else False
        if real and not library and not ejectable:
            continue
        volumes.append(
            {
                "id": str(path),
                "name": path.name,
                "path": str(path),
                "hasLibrary": library,
                "ejectable": ejectable,
                "rootName": _root_name(path),
            }
        )
    return volumes


def eject_volume(volume_id: str, volumes: list[dict] | None = None) -> None:
    """Eject a removable volume from the device list. `volumes` stands in for the live list in tests."""
    volume = Path(volume_id).resolve()
    listed = list_volumes() if volumes is None else volumes
    match = next((item for item in listed if Path(item["path"]).resolve() == volume), None)
    if match is None or not match.get("ejectable"):
        raise ValueError("That device can’t be ejected")
    try:
        subprocess.run(
            ["diskutil", "eject", str(volume)],
            check=True,
            timeout=30,
            capture_output=True,
            text=True,
        )
    except subprocess.CalledProcessError as err:
        detail = (err.stderr or err.stdout or "").strip()
        raise RuntimeError(detail or "Couldn’t eject the device") from err
    except subprocess.TimeoutExpired as err:
        raise RuntimeError("Ejecting the device took too long") from err
