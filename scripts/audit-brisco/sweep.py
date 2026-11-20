"""Paired, single-factor screen replay. Needs Pillow, numpy and the Rust app test executable."""
import argparse
import csv
import io
import json
import os
import subprocess
import tempfile
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
from PIL import Image, ImageDraw, ImageEnhance, ImageFilter, ImageOps
import numpy as np

ROOT = Path(__file__).resolve().parents[2]
FIX = ROOT / "crates/app/testdata"
SCENES = {
    "sito_briscola": dict(mano0=7, mano1=10, avversario=31, tua=26, briscola=8),
    "solitalian_briscola": dict(mano0=21, mano1=31, mano2=6, briscola=14),
}
# Ordered numeric levels. Baseline is the unmodified image for every scene.
LEVELS = {
    "zoom": [.2,.4,.7,1,1.3], "aspect_x": [.4,.7,1,1.3,1.6],
    "aspect_y": [.4,.7,1,1.3,1.6], "rotazione": [-15,-5,0,5,15],
    "offset_x": [-.35,-.15,0,.15,.35], "offset_y": [-.35,-.15,0,.15,.35],
    "bordo_x": [0,.05,.15,.3,.5], "bordo_y": [0,.05,.15,.3,.5],
    "luminosita": [.20,.55,1,1.45,2], "contrasto": [.2,.6,1,1.4,2],
    "saturazione": [0,.5,1,1.5,2], "gamma": [.3,.6,1,1.6,3],
    "tinta": [-120,-45,0,45,120], "sfocatura": [0,1,2,4,8],
    "nitidezza": [0,.5,1,2,4], "jpeg": [3,12,35,70,100],
    "rumore": [0,4,12,25,45], "posterizzazione": [2,3,4,6,8],
    "velo_bianco": [0,.2,.4,.6,.8], "oscuramento_rosso": [.15,.45,.7,.85,1],
    "oscuramento_blu": [.15,.45,.7,.85,1], "copertura_centrale": [0,.05,.12,.24,.45],
    "inclinazione_x": [-.28,-.1,0,.1,.28], "pixelatura": [1,2,4,8,16],
}
L = Image.Resampling.LANCZOS
def transform(im, key, value):
    w,h=im.size
    if key in ('zoom','aspect_x','aspect_y'):
        return im.resize((round(w*(1 if key=='aspect_y' else value)),round(h*(1 if key=='aspect_x' else value))),L)
    if key=='rotazione': return im.rotate(value,Image.Resampling.BICUBIC)
    if key.startswith('offset_') or key=='inclinazione_x':
        dx=w*value if key=='offset_x' else 0
        dy=h*value if key=='offset_y' else 0
        shear=value if key=='inclinazione_x' else 0
        return im.transform(im.size,Image.Transform.AFFINE,(1,shear,-dx-shear*h/2,0,1,-dy),Image.Resampling.BICUBIC)
    if key.startswith('bordo_'):
        return ImageOps.expand(im,(round(w*value) if key=='bordo_x' else 0,round(h*value) if key=='bordo_y' else 0),fill=(24,24,24))
    enh={'luminosita':ImageEnhance.Brightness,'contrasto':ImageEnhance.Contrast,'saturazione':ImageEnhance.Color,'nitidezza':ImageEnhance.Sharpness}
    if key in enh: return enh[key](im).enhance(value)
    if key=='gamma': return im.point([round(255*(x/255)**value) for x in range(256)]*3)
    if key=='tinta':
        hue,s,v=im.convert('HSV').split()
        return Image.merge('HSV',(hue.point([(i+round(value*255/360))%256 for i in range(256)]),s,v)).convert('RGB') if value else im
    if key=='sfocatura': return im.filter(ImageFilter.GaussianBlur(value))
    if key=='jpeg':
        buf=io.BytesIO()
        im.save(buf,'JPEG',quality=value)
        buf.seek(0)
        return Image.open(buf).convert('RGB')
    if key=='rumore':
        arr=np.asarray(im)
        noise=np.random.default_rng(431).normal(0,value,arr.shape[:2]).astype(np.int16)
        return Image.fromarray(np.clip(arr.astype(np.int16)+noise[:,:,None],0,255).astype(np.uint8))
    if key=='posterizzazione': return ImageOps.posterize(im,value)
    if key=='velo_bianco': return Image.blend(im,Image.new('RGB',im.size,'white'),value)
    if key.startswith('oscuramento_'):
        chan=0 if key.endswith('rosso') else 2
        return im.point([round(i*(value if c==chan else 1)) for c in range(3) for i in range(256)])
    if key=='copertura_centrale':
        if value:
            im=im.copy()
            ImageDraw.Draw(im).rectangle((round(w*(.5-value/2)),0,round(w*(.5+value/2)),h),fill=(65,66,71))
        return im
    if key=='pixelatura': return im.resize((max(1,w//value),max(1,h//value)),L).resize(im.size,Image.Resampling.NEAREST)
    raise ValueError(key)

def run(case):
    key,val,site=case
    path = FIX / (site + ('.jpg' if site == 'solitalian_mare' else '.png'))
    im=transform(Image.open(path).convert('RGB'),key,val)
    with tempfile.TemporaryDirectory() as tmp:
        d=Path(tmp)
        im.save(d/'frame.png',compress_level=1)
        manifest=d/'manifest.json'
        manifest.write_text(json.dumps(dict(scopa=False,frames=[dict(ms=1,image='frame.png',cards=SCENES[site])])))
        proc=subprocess.run([args.binary,'replay_carte_annotate','--ignored','--nocapture'],env={**os.environ,'BRIS_REPLAY_CARTE':str(manifest)},capture_output=True,text=True,check=True)
    return parse(proc.stdout, key, val, site, SCENES[site])

def parse(stdout, key, val, site, expected):
    cards=[]
    extras=us=None
    for line in stdout.splitlines():
        parts=line.split()
        if parts[:1]==['lettura']:
            _,_,role,truth,got,auto=parts
            cards.append((role,int(truth),int(got),int(auto)))
        if parts[:1]==['extra']: extras=int(parts[2])
        if parts[:1]==['tempo']: us=int(parts[2])
    assert len(cards)==len(expected) and extras is not None and us is not None
    return [[key,val,site,role,truth,got,auto,extras,us] for role,truth,got,auto in cards]

if __name__=='__main__':
    ap=argparse.ArgumentParser()
    ap.add_argument('binary',type=lambda p:str(Path(p).resolve()))
    ap.add_argument('out')
    ap.add_argument('--workers',type=int,default=3)
    ap.add_argument('--holdout',action='store_true',help='Terza schermata con i tre ruoli mano annotati; extra non valutabili')
    args=ap.parse_args()
    if args.holdout:
        SCENES = {"solitalian_mare": dict(mano0=3,mano1=38,mano2=20)}
    with open(args.out,'w',newline='') as f, ThreadPoolExecutor(args.workers) as pool:
        wr=csv.writer(f)
        wr.writerow(['factor','level','scene','role','truth','prediction','automatic','extra_slots_in_frame','frame_us'])
        for rows in pool.map(run,((k,v,s) for k,vs in LEVELS.items() for v in vs for s in SCENES)):
            wr.writerows(rows)
            f.flush()
