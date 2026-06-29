#!/usr/bin/env python3
"""Generate deterministic Tauri desktop icon assets from the approved AccordMesh artwork."""
from __future__ import annotations

import argparse
import io
import struct
import shutil
from pathlib import Path

from PIL import Image, ImageDraw

PNG_SIZES = {
    "32x32.png": 32,
    "128x128.png": 128,
    "128x128@2x.png": 256,
    "Square30x30Logo.png": 30,
    "Square44x44Logo.png": 44,
    "Square71x71Logo.png": 71,
    "Square89x89Logo.png": 89,
    "Square107x107Logo.png": 107,
    "Square142x142Logo.png": 142,
    "Square150x150Logo.png": 150,
    "Square284x284Logo.png": 284,
    "Square310x310Logo.png": 310,
    "StoreLogo.png": 50,
}

ICNS_TYPES = {
    16: b"icp4",
    32: b"icp5",
    64: b"icp6",
    128: b"ic07",
    256: b"ic08",
    512: b"ic09",
    1024: b"ic10",
}


def approved_master(source: Path) -> Image.Image:
    original = Image.open(source).convert("RGBA")
    if original.width != original.height:
        raise ValueError(f"Icon source must be square, got {original.size}")
    if original.width < 512:
        raise ValueError(f"Icon source must be at least 512 px, got {original.width}")

    # Preserve the approved artwork while adding macOS-safe transparent corners
    # and a modest optical margin so the symbol remains legible at Dock sizes.
    canvas_size = 1024
    artwork_size = 920
    artwork = original.resize((artwork_size, artwork_size), Image.Resampling.LANCZOS)
    radius = round(artwork_size * 0.205)
    mask = Image.new("L", (artwork_size, artwork_size), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        (0, 0, artwork_size - 1, artwork_size - 1),
        radius=radius,
        fill=255,
    )
    artwork.putalpha(mask)
    canvas = Image.new("RGBA", (canvas_size, canvas_size), (0, 0, 0, 0))
    offset = (canvas_size - artwork_size) // 2
    canvas.alpha_composite(artwork, (offset, offset))
    return canvas


def png_bytes(image: Image.Image, size: int) -> bytes:
    resized = image.resize((size, size), Image.Resampling.LANCZOS)
    output = io.BytesIO()
    resized.save(output, format="PNG", optimize=True)
    return output.getvalue()


def write_icns(master: Image.Image, destination: Path) -> None:
    chunks = []
    for size, chunk_type in ICNS_TYPES.items():
        payload = png_bytes(master, size)
        chunks.append(chunk_type + struct.pack(">I", len(payload) + 8) + payload)
    body = b"".join(chunks)
    destination.write_bytes(b"icns" + struct.pack(">I", len(body) + 8) + body)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()

    args.output.mkdir(parents=True, exist_ok=True)
    master = approved_master(args.source)
    master.save(args.output / "icon.png", format="PNG", optimize=True)
    shutil.copyfile(args.source, args.output / "icon-source.png")

    for filename, size in PNG_SIZES.items():
        resized = master.resize((size, size), Image.Resampling.LANCZOS)
        resized.save(args.output / filename, format="PNG", optimize=True)

    ico = master.resize((256, 256), Image.Resampling.LANCZOS)
    ico.save(
        args.output / "icon.ico",
        format="ICO",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    write_icns(master, args.output / "icon.icns")


if __name__ == "__main__":
    main()
