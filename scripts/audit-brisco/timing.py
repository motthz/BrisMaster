from pathlib import Path
import sys,os,csv,subprocess,statistics
import sweep
from types import SimpleNamespace
import argparse
ap=argparse.ArgumentParser()
ap.add_argument('before',type=lambda s:str(Path(s).resolve()))
ap.add_argument('after',type=lambda s:str(Path(s).resolve()))
ap.add_argument('out',type=Path)
args=ap.parse_args()
cases=[('zoom',1,'sito_briscola'),('zoom',1,'solitalian_briscola'),('luminosita',.2,'sito_briscola'),('contrasto',.2,'solitalian_briscola'),('aspect_x',.4,'sito_briscola'),('aspect_y',1.6,'solitalian_briscola')]
with args.out.open('w') as f:
 w=csv.writer(f); w.writerow(['versione','ripetizione','misura','caso','valore','us'])
 for repeat,order in enumerate([['prima','dopo'],['dopo','prima'],['prima','dopo']]):
  for ver in order:
   sweep.args=SimpleNamespace(binary={'prima':args.before,'dopo':args.after}[ver])
   for case in cases:
    rows=sweep.run(case);w.writerow([ver,repeat,'lettura',case[2]+'/'+case[0],case[1],rows[0][-1]])
   out=subprocess.check_output([sweep.args.binary,'measure_advice_time','--ignored','--nocapture'],text=True)
   count=0
   for l in out.splitlines():
    if l.startswith('motore,'):
     _,seed,played,hand,us=l.split(',');w.writerow([ver,repeat,'motore',seed+'/'+played,hand,us]);count+=1
   assert count==76,count
   f.flush()
