import plistlib
import shutil
import subprocess
from pathlib import Path

from .paths import ROOT_NAMES, has_library


def _disk_info(path: Path) -> dict:
    try:
        raw = subprocess.check_output(["diskutil", "info", "-plist", str(path)], timeout=3, stderr=subprocess.DEVNULL)
        info = plistlib.loads(raw)
    except (OSError, subprocess.SubprocessError, plistlib.InvalidFileException):
        return {}
    return info if isinstance(info, dict) else {}


def _ejectable(info: dict) -> bool:
    return bool(info.get("RemovableMedia") or info.get("Ejectable")) and not info.get("Internal")


def _usage(path: Path) -> tuple[int, int, int]:
    try:
        usage = shutil.disk_usage(path)
    except OSError:
        return 0, 0, 0
    return int(usage.total), int(usage.used), int(usage.free)


def _fat_label(name: str) -> str:
    cleaned = "".join(ch if ch.isalnum() else "_" for ch in name.upper())
    cleaned = cleaned.strip("_") or "USB"
    return cleaned[:11]


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
        info = _disk_info(path) if real else {}
        ejectable = _ejectable(info) if real else False
        if real and not library and not ejectable:
            continue
        total, used, free = _usage(path)
        file_system = ""
        if real:
            file_system = str(info.get("FilesystemName") or info.get("FilesystemType") or "")
        volumes.append(
            {
                "id": str(path),
                "name": path.name,
                "path": str(path),
                "hasLibrary": library,
                "ejectable": ejectable,
                "rootName": _root_name(path),
                "fileSystem": file_system,
                "bytesTotal": total,
                "bytesUsed": used,
                "bytesFree": free,
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


def format_volume(volume_id: str, volumes: list[dict] | None = None) -> None:
    """Erase a removable volume and format it FAT32. `volumes` stands in for the live list in tests."""
    volume = Path(volume_id).resolve()
    listed = list_volumes() if volumes is None else volumes
    match = next((item for item in listed if Path(item["path"]).resolve() == volume), None)
    if match is None or not match.get("ejectable"):
        raise ValueError("That device can’t be formatted")
    info = _disk_info(volume)
    if not _ejectable(info):
        raise ValueError("That device can’t be formatted")
    device = str(info.get("DeviceIdentifier") or "")
    if not device:
        raise RuntimeError("Couldn’t identify the device")
    try:
        subprocess.run(
            ["diskutil", "eraseVolume", "FAT32", _fat_label(str(match.get("name") or volume.name)), device],
            check=True,
            timeout=120,
            capture_output=True,
            text=True,
        )
    except subprocess.CalledProcessError as err:
        detail = (err.stderr or err.stdout or "").strip()
        raise RuntimeError(detail or "Couldn’t format the device") from err
    except subprocess.TimeoutExpired as err:
        raise RuntimeError("Formatting the device took too long") from err
