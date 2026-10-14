from pathlib import Path

ROOT_NAMES = ("PIONEER", ".PIONEER")
DB_FILES = ("export.pdb", "exportLibrary.db")


def pioneer_dir(volume: Path) -> tuple[Path, str]:
    """Return the device root, keeping a spelling the stick already uses."""
    found: list[tuple[Path, str]] = []
    for name in ROOT_NAMES:
        root = volume / name
        rekordbox = root / "rekordbox"
        if any((rekordbox / name).is_file() for name in DB_FILES) or root.is_dir():
            found.append((root, name))
    for root, name in found:
        rekordbox = root / "rekordbox"
        if any((rekordbox / db).is_file() for db in DB_FILES):
            return root, name
    if found:
        return found[0]
    return volume / "PIONEER", "PIONEER"


def has_library(volume: Path) -> bool:
    for name in ROOT_NAMES:
        rekordbox = volume / name / "rekordbox"
        if any((rekordbox / db).is_file() for db in DB_FILES):
            return True
    return False


def inside(volume: Path, path: Path) -> Path:
    root = volume.resolve()
    target = path.resolve()
    if target != root and root not in target.parents:
        raise ValueError(f"Path escapes the volume: {path}")
    return target


def volume_path(relative: str, volume: Path) -> Path:
    text = relative.replace("\\", "/")
    if text.startswith("/"):
        text = text[1:]
    if not text or text.startswith("/") or any(part in ("", ".", "..") for part in text.split("/")):
        raise ValueError(f"Unsafe device path: {relative}")
    return inside(volume, volume / text)


def analysis_dir(audio_path: str, root_name: str) -> str:
    """Directory for ANLZ companions, from the final leading-slash audio path."""
    digest = 0
    encoded = audio_path.encode("utf-16-le")
    for index in range(0, len(encoded), 2):
        unit = int.from_bytes(encoded[index : index + 2], "little")
        if unit == 0:
            break
        digest = (digest * 0x34F5501D + unit * 0x93B6) & 0xFFFFFFFF
    hashed = digest % 0x30D43
    shard = 0
    for bit, source in enumerate((0, 2, 6, 7, 9, 13, 16)):
        if hashed & (1 << source):
            shard |= 1 << bit
    return f"/{root_name}/USBANLZ/P{shard:03X}/{hashed:08X}"
