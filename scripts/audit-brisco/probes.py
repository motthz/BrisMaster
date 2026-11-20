"""Replays generated card fixtures, negative controls, combinations and proposed restorations."""
import argparse
import csv
import json
import os
from pathlib import Path
import subprocess
import tempfile
from concurrent.futures import ThreadPoolExecutor
from PIL import Image,ImageDraw,ImageOps,ImageFilter
from sweep import FIX,SCENES,LEVELS,transform,parse

FIELDS = ['factor','level','scene','role','truth','prediction','automatic','extra_slots_in_frame','frame_us']
def evaluate(im, expected, key, value, site):
    with tempfile.TemporaryDirectory() as temp:
        d=Path(temp)
        im.save(d/'frame.png',compress_level=1)
        path=d/'manifest.json'
        path.write_text(json.dumps({'scopa':False,'frames':[{'ms':1,'image':'frame.png','cards':expected}]}))
        proc=subprocess.run([args.binary,'replay_carte_annotate','--ignored','--nocapture'],env={**os.environ,'BRIS_REPLAY_CARTE':str(path)},capture_output=True,text=True,check=True)
        if expected: return parse(proc.stdout,key,value,site,expected)
        lines=proc.stdout.splitlines()
        def metric(name): return int(next(line.split()[2] for line in lines if line.startswith(name+' ')))
        return [[key,value,site,'',-1,-1,0,metric('extra'),metric('tempo')]]
def run(case):
    kind,key,value,site=case
    if kind=='fixture':
        item=index[key]
        return evaluate(Image.open(args.fixtures/item['image']),item['cards'],item['factor'],item['level'],item['scene'])
    if kind=='negative':
        im=Image.new('RGB',(720,480),(39,106,69))
        draw=ImageDraw.Draw(im)
        for x,y in ((200,180),(270,365),(335,365),(400,365)):
            draw.rounded_rectangle((x,y,x+48,y+75),radius=4,fill='#d4cdbf')
            for k in range(4): draw.rectangle((x+6,y+10*k+6,x+40-k*3,y+10*k+9),fill=['#444444','#606064','#152e96','#a07840'][k])
        return evaluate(transform(im,key,value),{},key,value,'interfaccia-vuota')
    im=Image.open(FIX/(site+'.png')).convert('RGB')
    if kind=='combined':
        transforms=interactions[key][value]
        for f,v in transforms: im=transform(im,f,v)
    else:
        factor,variant=key.split('/')
        im=transform(im,factor,value)
        if variant=='unsharp': im=im.filter(ImageFilter.UnsharpMask(2,175,3))
        elif variant=='median': im=im.filter(ImageFilter.MedianFilter(3))
        elif variant=='grigio': im=ImageOps.grayscale(im).convert('RGB')
    return evaluate(im,SCENES[site],key,value,site)
interactions={
    'piccolo-buio':[[('zoom',s),('luminosita',b)] for s,b in [(1,1),(.7,.75),(.5,.5),(.35,.35),(.2,.2)]],
    'velo-jpeg':[[('velo_bianco',s),('jpeg',q)] for s,q in [(0,100),(.2,70),(.4,35),(.6,12),(.8,3)]],
    'stretto-rumore':[[('aspect_x',s),('rumore',n)] for s,n in [(1,0),(.85,4),(.7,12),(.55,25),(.4,45)]],
    'zoom-copertura':[[('zoom',s),('copertura_centrale',c)] for s,c in [(1,0),(.8,.05),(.6,.12),(.4,.24),(.2,.45)]],
}
if __name__=='__main__':
    ap=argparse.ArgumentParser()
    ap.add_argument('binary')
    ap.add_argument('mode',choices=['fixtures','controls','alternatives'])
    ap.add_argument('out')
    ap.add_argument('--fixtures',type=Path)
    args=ap.parse_args()
    if args.mode=='fixtures':
        index=json.loads((args.fixtures/'index.json').read_text())
        cases=[('fixture',i,0,'') for i in range(len(index))]
    elif args.mode=='controls':
        cases=[('negative',f,v,'') for f in LEVELS for v in [LEVELS[f][0],LEVELS[f][-1]]]
        cases += [('combined',k,v,s) for k in interactions for v in range(5) for s in SCENES]
    else:
        experiments={'unsharp':['sfocatura','pixelatura','nitidezza','inclinazione_x','rotazione'],
                     'median':['rumore','jpeg','posterizzazione','gamma'],
                     'grigio':['tinta','luminosita','contrasto','saturazione','oscuramento_rosso','oscuramento_blu']}
        cases=[('trial',f+'/'+variant,v,s) for variant,factors in experiments.items() for f in factors for v in [LEVELS[f][0],LEVELS[f][-1]] for s in SCENES]
    with open(args.out,'w',newline='') as fd,ThreadPoolExecutor(3) as pool:
        wr=csv.writer(fd)
        wr.writerow(FIELDS)
        for rows in pool.map(run,cases):wr.writerows(rows); fd.flush()
