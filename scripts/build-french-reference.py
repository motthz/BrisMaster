#!/usr/bin/env python3
"""Rebuild the bundled reference sheet (optional developer tool; requires Pillow)."""
from concurrent.futures import ThreadPoolExecutor
import hashlib
import io
import json
from pathlib import Path
from urllib.request import urlopen
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / 'crates/app/assets'
REVISION = '1e4497c05c3da9956c9f517bd386e9a7090ff7fa'
BASE = f'https://raw.githubusercontent.com/hayeah/playing-cards-assets/{REVISION}/png/'
HASHES = json.loads((ASSETS / 'FRENCH-CARDS-SHA256.json').read_text())
RANKS = ['ace', '2', '3', '4', '5', '6', '7', '8', '9', '10', 'jack', 'queen', 'king']
NAMES = [f'{rank}_of_{suit}.png' for suit in ['hearts','diamonds','clubs','spades'] for rank in RANKS]
NAMES += ['black_joker.png', 'red_joker.png']

def fetch(name):
    with urlopen(BASE + name, timeout=30) as response:
        raw = response.read()
    if hashlib.sha256(raw).hexdigest() != HASHES[name]:
        raise ValueError(f'Checksum mismatch for {name}; reference sheet unchanged')
    card = Image.open(io.BytesIO(raw)).convert('RGBA')
    background = Image.new('RGBA', card.size, 'white')
    background.alpha_composite(card)
    return background.convert('RGB').resize((100,160), Image.Resampling.LANCZOS)

if __name__ == '__main__':
    with ThreadPoolExecutor(max_workers=6) as pool:
        cards = list(pool.map(fetch, NAMES))
    sheet = Image.new('RGB', (1300,800), 'white')
    for i, card in enumerate(cards):
        sheet.paste(card, ((i % 13)*100, (i // 13)*160))
    sheet.save(ASSETS / 'cartes_francesi.png', optimize=True)
    print('Verified and rebuilt 54 reference images (53 game kinds).')
