"""Prepara due mazzi esterni per il test mazzi_sconosciuti (richiede Pillow)."""
import argparse
from io import BytesIO
from pathlib import Path
from urllib.request import urlopen
from zipfile import ZipFile

from PIL import Image


SUITS = ("denari", "coppe", "spade", "bastoni")
RANKS = ("asso", "due", "tre", "quattro", "cinque", "sei", "sette", "fante", "cavallo", "re")
SNAPSHOTS = {
    "piacentine": ("nico33t/briscola", "60784e4ab4a2a75e668ce21d82ffc75738f18181"),
    "bresciane": ("mhamilt/Italian-decks", "97c6e6c2c322722ebfc419a56f0d2a0d62d810ed"),
}


def prepare(root, name, repo, revision):
    dest = root / name
    dest.mkdir(parents=True, exist_ok=True)
    with urlopen(f"https://codeload.github.com/{repo}/zip/{revision}", timeout=90) as response:
        archive = BytesIO(response.read())
    with ZipFile(archive) as cards:
        prefix = f"{repo.split('/')[1]}-{revision}/"
        for suit in range(4):
            for rank in range(10):
                if name == "piacentine":
                    path = f"src/assets/cards/{SUITS[suit]}-{RANKS[rank]}.webp"
                else:
                    number = rank + (1 if rank < 7 else 4)
                    path = f"decks/bresciane/{'dcsb'[suit]}{number}.png"
                with Image.open(BytesIO(cards.read(prefix + path))) as original:
                    rgba = original.convert("RGBA")
                    output = Image.new("RGB", rgba.size, "white")
                    output.paste(rgba, mask=rgba.getchannel("A"))
                    output.save(dest / f"{suit * 10 + rank:02}.png")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cartella", type=Path)
    root = parser.parse_args().cartella
    for name, (repo, revision) in SNAPSHOTS.items():
        prepare(root, name, repo, revision)
