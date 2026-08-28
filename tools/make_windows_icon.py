from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

root = Path(__file__).parents[1]
icons = root / "apps" / "desktop" / "src-tauri" / "icons"
icons.mkdir(parents=True, exist_ok=True)
png = icons / "icon.png"
ico = icons / "icon.ico"

size = 256
image = Image.new("RGBA", (size, size), (8, 12, 24, 255))
draw = ImageDraw.Draw(image)
draw.rounded_rectangle((12, 12, size - 12, size - 12), radius=52, fill=(20, 35, 68, 255), outline=(76, 220, 210, 255), width=6)
draw.ellipse((58, 58, 198, 198), fill=(12, 22, 44, 255), outline=(111, 93, 255, 255), width=8)
try:
    font = ImageFont.truetype("DejaVuSans-Bold.ttf", 72)
except OSError:
    font = ImageFont.load_default()
text = "N"
bbox = draw.textbbox((0, 0), text, font=font)
position = ((size - (bbox[2] - bbox[0])) // 2, (size - (bbox[3] - bbox[1])) // 2 - 8)
draw.text(position, text, font=font, fill=(240, 247, 255, 255), stroke_width=1, stroke_fill=(76, 220, 210, 255))
image.save(png, format="PNG")
image.save(ico, format="ICO", sizes=[(256, 256), (128, 128), (64, 64), (48, 48), (32, 32), (16, 16)])
print(ico)
