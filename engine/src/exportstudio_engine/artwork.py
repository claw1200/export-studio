import io
from pathlib import Path

from PIL import Image, ImageOps

SIZES = {"m": 240, "s": 80}
JPEG_QUALITY = 90


def save(data: bytes, share_dir: Path, uuid: str) -> str:
    """Write a cover and its rekordbox thumbnails under share/PIONEER/Artwork. Returns the stored ImagePath."""
    with Image.open(io.BytesIO(data)) as image:
        image.load()
        fmt = (image.format or "").upper()
        if fmt not in ("JPEG", "PNG"):
            raise ValueError(f"Unsupported artwork format: {fmt or 'unknown'}")
        ext = "jpg" if fmt == "JPEG" else "png"
        relative = f"/PIONEER/Artwork/{uuid[:3]}/{uuid}/artwork.{ext}"
        folder = share_dir / relative.lstrip("/").rsplit("/", 1)[0]
        folder.mkdir(parents=True, exist_ok=True)
        (folder / f"artwork.{ext}").write_bytes(data)
        rgb = image.convert("RGB")
        for suffix, size in SIZES.items():
            thumb = ImageOps.fit(rgb, (size, size), Image.Resampling.LANCZOS)
            thumb.save(folder / f"artwork_{suffix}.jpg", "JPEG", quality=JPEG_QUALITY)
    return relative
