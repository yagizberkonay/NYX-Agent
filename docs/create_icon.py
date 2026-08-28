from PIL import Image, ImageDraw

size = 512
image = Image.new("RGBA", (size, size), (23, 20, 18, 255))
draw = ImageDraw.Draw(image)
center = size // 2
for radius in range(175, 20, -3):
    ratio = (radius - 20) / 155
    color = (int(233 * (1 - ratio) + 80 * ratio), int(140 * (1 - ratio) + 53 * ratio), int(77 * (1 - ratio) + 31 * ratio), 255)
    draw.ellipse((center - radius, center - radius, center + radius, center + radius), fill=color)
draw.ellipse((145, 145, 367, 367), fill=(35, 29, 25, 255), outline=(244, 239, 232, 80), width=3)
draw.text((center - 48, center - 14), "NYX", fill=(244, 239, 232, 230))
image.save("apps/desktop/src-tauri/icons/icon.png")
