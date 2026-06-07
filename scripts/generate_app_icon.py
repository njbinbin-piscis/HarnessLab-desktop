#!/usr/bin/env python3
"""Generate HarnessLab app icon — matches frontend .brand .logo (H on gradient)."""

from __future__ import annotations

import math
import struct
import subprocess
import sys
import zlib
from pathlib import Path

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    sys.exit("Pillow required: pip install Pillow")

ROOT = Path(__file__).resolve().parents[1]
ICONS = ROOT / "src-tauri" / "icons"
SIZE = 1024
RADIUS_RATIO = 7 / 26  # same as UI: 7px on 26px
ACCENT = (0x5B, 0x8C, 0xFF)
ACCENT2 = (0x38, 0xD3, 0x9F)
LETTER = (0x06, 0x10, 0x18)


def lerp(a: int, b: int, t: float) -> int:
    return int(a + (b - a) * t)


def gradient_pixel(x: int, y: int, w: int, h: int) -> tuple[int, int, int, int]:
    # 135deg: top-left accent -> bottom-right accent2
    t = (x / max(w - 1, 1) + y / max(h - 1, 1)) / 2.0
    t = max(0.0, min(1.0, t))
    return (
        lerp(ACCENT[0], ACCENT2[0], t),
        lerp(ACCENT[1], ACCENT2[1], t),
        lerp(ACCENT[2], ACCENT2[2], t),
        255,
    )


def rounded_rect_mask(w: int, h: int, radius: float) -> Image.Image:
    mask = Image.new("L", (w, h), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle((0, 0, w - 1, h - 1), radius=radius, fill=255)
    return mask


def load_bold_font(size: int) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    candidates = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        "/usr/share/fonts/truetype/inter/Inter-Bold.ttf",
        "/usr/share/fonts/opentype/inter/Inter-Bold.otf",
    ]
    for path in candidates:
        if Path(path).exists():
            return ImageFont.truetype(path, size)
    return ImageFont.load_default()


def render_icon(size: int = SIZE) -> Image.Image:
    radius = size * RADIUS_RATIO
    bg = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    grad = Image.new("RGBA", (size, size))
    px = grad.load()
    for y in range(size):
        for x in range(size):
            px[x, y] = gradient_pixel(x, y, size, size)

    mask = rounded_rect_mask(size, size, radius)
    bg.paste(grad, (0, 0), mask)

    draw = ImageDraw.Draw(bg)
    font_size = int(size * 0.58)
    font = load_bold_font(font_size)
    letter = "H"
    bbox = draw.textbbox((0, 0), letter, font=font)
    tw, th = bbox[2] - bbox[0], bbox[3] - bbox[1]
    x = (size - tw) / 2 - bbox[0]
    y = (size - th) / 2 - bbox[1] - size * 0.02
    draw.text((x, y), letter, fill=LETTER + (255,), font=font)
    return bg


def write_png(path: Path, img: Image.Image) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    img.save(path, format="PNG")


def write_ico(path: Path, sizes: list[int]) -> None:
    """Minimal multi-size ICO writer (PNG-compressed entries)."""
    images = [render_icon(s).convert("RGBA") for s in sizes]

    def png_bytes(im: Image.Image) -> bytes:
        import io

        buf = io.BytesIO()
        im.save(buf, format="PNG")
        return buf.getvalue()

    entries = []
    for im, dim in zip(images, sizes):
        data = png_bytes(im)
        entries.append((dim, data))

    # ICO header + directory
    offset = 6 + 16 * len(entries)
    parts = [struct.pack("<HHH", 0, 1, len(entries))]
    for dim, data in entries:
        w = 0 if dim >= 256 else dim
        h = 0 if dim >= 256 else dim
        parts.append(
            struct.pack(
                "<BBBBHHII",
                w,
                h,
                0,
                0,
                1,
                32,
                len(data),
                offset,
            )
        )
        offset += len(data)
    for _, data in entries:
        parts.append(data)
    path.write_bytes(b"".join(parts))


def main() -> None:
    master = render_icon(SIZE)
    write_png(ICONS / "icon.png", master)
    write_png(ICONS / "32x32.png", master.resize((32, 32), Image.Resampling.LANCZOS))
    write_png(ICONS / "128x128.png", master.resize((128, 128), Image.Resampling.LANCZOS))
    write_png(ICONS / "128x128@2x.png", master.resize((256, 256), Image.Resampling.LANCZOS))
    write_ico(ICONS / "icon.ico", [16, 32, 48, 64, 128, 256])

    # macOS .icns via iconutil if on mac; on Linux try png2icns or skip
    icns = ICONS / "icon.icns"
    if sys.platform == "darwin":
        subprocess.run(
            ["iconutil", "-c", "icns", "-o", str(icns), str(ICONS / "icon.iconset")],
            check=False,
        )
    else:
        # png2icns from icnsutils, or leave 512 PNG for tauri bundler fallback
        png512 = ICONS / "icon.png"
        try:
            subprocess.run(
                ["png2icns", str(icns), str(png512)],
                check=True,
                capture_output=True,
            )
        except (FileNotFoundError, subprocess.CalledProcessError):
            # Store 512 master; tauri build on mac will regenerate icns if needed
            write_png(ICONS / "512x512.png", master.resize((512, 512), Image.Resampling.LANCZOS))

    print(f"Wrote HarnessLab icons to {ICONS}")


if __name__ == "__main__":
    main()
