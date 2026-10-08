from urllib.parse import quote, unquote


def track_key(volume: str, export_id: int) -> str:
    return f"usb:{quote(volume, safe='')}:{export_id}"


def parse_track_key(key: str) -> tuple[str, int]:
    if not key.startswith("usb:"):
        raise ValueError("Not a USB track")
    volume, _, raw = key[4:].partition(":")
    if not volume or not raw:
        raise ValueError("Not a USB track")
    return unquote(volume), int(raw)
