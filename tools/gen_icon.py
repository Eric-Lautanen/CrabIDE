"""Generate icon_data.rs from assets/icon-32.png."""
from PIL import Image
import os, sys

src_dir = os.path.join(os.path.dirname(__file__), "..")
png_path = os.path.join(src_dir, "assets", "icon-32.png")
out_path = os.path.join(src_dir, "crates", "crabide-app", "src", "icon_data.rs")

img = Image.open(png_path).convert("RGBA")
assert img.size == (32, 32), f"Expected 32x32, got {img.size}"
rgba = list(img.tobytes())
assert len(rgba) == 4096, f"Expected 4096 bytes, got {len(rgba)}"

lines = [
    "// Auto-generated from assets/icon-32.png. Run `python tools/gen_icon.py` to regenerate.",
    "pub(crate) static ICON_RGBA: &[u8] = &[",
]
for i in range(0, len(rgba), 16):
    chunk = rgba[i : i + 16]
    lines.append("    " + ", ".join(f"{b:3}u8" for b in chunk) + ",")
lines.append("];")
lines.append("pub(crate) const ICON_WIDTH: u32 = 32;")
lines.append("pub(crate) const ICON_HEIGHT: u32 = 32;")

os.makedirs(os.path.dirname(out_path), exist_ok=True)
with open(out_path, "w") as f:
    f.write("\n".join(lines) + "\n")

print(f"Generated {out_path} ({len(rgba)} bytes, {len(lines)} lines)")
