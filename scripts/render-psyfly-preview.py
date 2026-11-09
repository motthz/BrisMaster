"""Render the actual egui meshes exported by Psy Fly's manual preview test.

Requires numpy and Pillow. This previews drawing and animation, not native Windows
placement, capture exclusion, transparency, or input passthrough.
"""

import argparse
import json
from pathlib import Path

import numpy as np
from PIL import Image


def update_textures(updates, textures, directory):
    for update in updates:
        pixels = np.array(Image.open(directory / update["path"]), dtype=np.float32) / 255
        if update["pos"] is None:
            textures[update["id"]] = pixels
        else:
            x, y = update["pos"]
            textures[update["id"]][y:y + pixels.shape[0], x:x + pixels.shape[1]] = pixels


def render_mesh(canvas, mesh, textures):
    vertices = np.array(mesh["vertices"], dtype=np.float32)
    atlas = textures[mesh["texture"]]
    height, width = atlas.shape[:2]
    clip = mesh["clip"]
    for indices in np.array(mesh["indices"]).reshape(-1, 3):
        triangle = vertices[indices]
        a, b, c = triangle[:, :2]
        determinant = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
        if abs(float(determinant)) < 1e-7:
            continue
        x0 = max(0, int(np.floor(min(triangle[:, 0]))), int(clip[0]))
        x1 = min(canvas.shape[1], int(np.ceil(max(triangle[:, 0]))), int(clip[2]))
        y0 = max(0, int(np.floor(min(triangle[:, 1]))), int(clip[1]))
        y1 = min(canvas.shape[0], int(np.ceil(max(triangle[:, 1]))), int(clip[3]))
        if x1 <= x0 or y1 <= y0:
            continue
        yy, xx = np.mgrid[y0:y1, x0:x1]
        px, py = xx + 0.5, yy + 0.5
        w1 = ((px - a[0]) * (c[1] - a[1]) - (py - a[1]) * (c[0] - a[0])) / determinant
        w2 = ((b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (px - a[0])) / determinant
        w0 = 1 - w1 - w2
        mask = (w0 >= 0) & (w1 >= 0) & (w2 >= 0)
        if not mask.any():
            continue
        weights = np.stack((w0[mask], w1[mask], w2[mask]), axis=-1)
        uv = weights @ triangle[:, 2:4]
        color = (weights @ triangle[:, 4:8]) / 255
        texel = atlas[
            np.clip((uv[:, 1] * height).astype(int), 0, height - 1),
            np.clip((uv[:, 0] * width).astype(int), 0, width - 1),
        ]
        source = texel * color
        destination = canvas[y0:y1, x0:x1]
        destination[mask] = source + destination[mask] * (1 - source[:, 3:4])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path, help="Export directory with frames.json and atlas PNGs")
    parser.add_argument("output", type=Path, help="Directory for the GIF and contact sheet")
    parser.add_argument("--stride", type=int, default=1, help="Render every nth frame")
    args = parser.parse_args()
    if args.stride < 1:
        parser.error("stride must be positive")
    args.output.mkdir(parents=True, exist_ok=True)
    frames = json.loads((args.input / "frames.json").read_text())
    textures, images, samples = {}, [], []
    for index, frame in enumerate(frames):
        update_textures(frame["textures"], textures, args.input)
        is_sample = index % 36 == 18
        if index % args.stride and not is_sample:
            continue
        canvas = np.zeros((400, 640, 4), dtype=np.float32)
        for mesh in frame["meshes"]:
            render_mesh(canvas, mesh, textures)
        image = Image.fromarray((np.clip(canvas, 0, 1) * 255).astype(np.uint8)).convert("RGB")
        if index % args.stride == 0:
            images.append(image)
        if is_sample:
            samples.append(image)
        if index % 36 == 0:
            print(f"Rendering {frame['label']}", flush=True)
    images[0].save(
        args.output / "psyfly-animations.gif", save_all=True, append_images=images[1:],
        duration=50 * args.stride, loop=0, optimize=True,
    )
    gallery = Image.new("RGB", (1280, 1200), (19, 34, 32))
    for index, image in enumerate(samples):
        gallery.paste(image, ((index % 2) * 640, (index // 2) * 400))
    gallery.save(args.output / "psyfly-gallery.png")


if __name__ == "__main__":
    main()
