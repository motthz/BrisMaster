"""Build the paired PDF curves and machine-readable tables from the audit archive."""
import argparse
import json
import re
import textwrap
from pathlib import Path
import numpy as np
import pandas as pd
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.backends.backend_pdf import PdfPages
from matplotlib.lines import Line2D

ap=argparse.ArgumentParser()
ap.add_argument('data',type=Path)
ap.add_argument('output',type=Path)
opt=ap.parse_args()
src=opt.data
out=opt.output
out.mkdir(parents=True,exist_ok=True)
colors=['#30619a','#c45624']
titles=['0.6.13','Candidata']
def read(name): return pd.read_csv(src/(name+'.csv'))
def primary(end):
    return pd.concat([read(n) for n in (end,'holdout-'+end,'carte-'+end)],ignore_index=True)
def score(x):
    x=x[x.truth>=0].copy()
    x['ok']=x.truth==x.prediction
    x['safe']=x.ok & x.automatic.eq(1)
    x['bad']=~x.ok & x.automatic.eq(1)
    x['found']=x.prediction>=0
    x['suit']=x.found & ((x.prediction//10)==(x.truth//10))
    x['rank']=x.found & ((x.prediction%10)==(x.truth%10))
    return x
data=[score(primary(n)) for n in ('prima','finale')]
mix=[score(read('misti-'+n)) for n in ('prima','finale')]
games=[]
for n in ['prima','finale']:
    rows=[]
    for prefix in ['partite','riserva']:
        for line in (src/(prefix+'-'+n+'.log')).read_text().splitlines():
            m=re.match(r'seed=(\d+) scenario=(\w+) punti_veri=\[(\d+), (\d+)\] (\{.*\})',line)
            if m:
                row=json.loads(m[5]);row.update(seed=int(m[1]),scenario=m[2],point_error=sum(abs(x-int(v)) for x,v in zip(row['points'],[m[3],m[4]])))
                rows.append(row)
    g=pd.DataFrame(rows)
    assert len(g)==44 and not g.duplicated(['seed','scenario']).any()
    games.append(g)
assert [len(x) for x in data]==[1896,1896] and [len(x) for x in mix]==[180,180]
keys=['factor','level','scene','role','truth']
merged=data[0].merge(data[1],on=keys,validate='one_to_one',suffixes=('_prima','_dopo'))
merged.to_csv(out/'confronto-per-carta.csv',index=False)
frames=['factor','level','scene']
def framify(x):
    return x.groupby(frames,as_index=False).agg(esatta=('ok','all'),certe=('safe','all'),ms=('frame_us',lambda t:t.iloc[0]/1000),extra=('extra_slots_in_frame','first'))
tim=pd.read_csv(src/'tempi.csv')
factors={
'zoom':('Scala uniforme','×'), 'aspect_x':('Scala orizzontale','×'), 'aspect_y':('Scala verticale','×'),
'rotazione':('Rotazione del viewport','°'),'offset_x':('Traslazione orizzontale','frazione W'),
'offset_y':('Traslazione verticale','frazione H'),'bordo_x':('Bordo laterale per lato','frazione W'),
'bordo_y':('Bordo verticale per lato','frazione H'),'luminosita':('Luminosità RGB','×'),
'contrasto':('Contrasto globale','×'),'saturazione':('Saturazione','×'),'gamma':('Gamma','esponente'),
'tinta':('Rotazione della tinta','°'),'sfocatura':('Sfocatura gaussiana','raggio px'),
'nitidezza':('Nitidezza','×'),'jpeg':('Qualità JPEG','qualità'),'rumore':('Rumore gaussiano','σ su 255'),
'posterizzazione':('Profondità per canale','bit'),'velo_bianco':('Opacità del velo bianco','frazione'),
'oscuramento_rosso':('Guadagno del rosso','×'),'oscuramento_blu':('Guadagno del blu','×'),
'copertura_centrale':('Fascia centrale coperta','frazione W'),'inclinazione_x':('Taglio orizzontale','Δx/Δy'),
'pixelatura':('Pixelatura','fattore'), 'altezza_carta':('Altezza della carta','px'),
'forma_carta':('Proporzioni della carta','W/H'),'ventaglio':('Apertura del ventaglio','°'),
'copertura_briscola':('Briscola coperta dal tallone','frazione'),'scala_tallone':('Dimensione del tallone','×'),
'opacita_logo':('Opacità del logo','frazione'),'grafica_fondo':('Grafica del fondo','categoria'),
'grafica_mazzo':('Famiglia grafica del mazzo','categoria')}
categories={'grafica_fondo':['Verde','Chiaro','Legno','Mare','Cielo'],'grafica_mazzo':['Wiki','Classico','Después']}
assert set(factors)==set(data[0].factor)
plt.rcParams.update({'font.family':'DejaVu Sans','font.size':10,'axes.spines.top':False,'axes.spines.right':False,'axes.labelcolor':'#344454','axes.edgecolor':'#bcc6ce','text.color':'#142536','figure.facecolor':'white','pdf.fonttype':42})
def page(title,sub):
    f=plt.figure(figsize=(11.69,8.27))
    f.text(.06,.949,title,fontsize=22,weight='bold')
    f.text(.06,.907,sub,fontsize=10,color='#536779')
    return f
def save(f):
    global num
    num+=1
    f.text(.06,.028,'BrisMaster  ·  Audit del riconoscimento della Briscola  ·  20 novembre 2026',fontsize=8,color='#536779')
    f.text(.946,.028,str(num),fontsize=8,ha='right')
    pdf.savefig(f);plt.close(f)
def narrative(title,sub,sections):
    f=page(title,sub)
    y=.835
    for h,txt in sections:
        f.text(.06,y,h,fontsize=14,weight='bold',color='#175b65')
        lines=textwrap.wrap(txt,115,break_long_words=False)
        f.text(.06,y-.032,'\n'.join(lines),fontsize=11,va='top',linespacing=1.45)
        y-=.065+.029*len(lines)
    save(f)
def tab(title,sub,headers,rows,widths,fs=10):
    f=page(title,sub);ax=f.add_axes([.058,.09,.887,.765]);ax.axis('off')
    t=ax.table(cellText=rows,colLabels=headers,colWidths=widths,cellLoc='left',colLoc='left',bbox=[0,0,1,1])
    t.auto_set_font_size(False);t.set_fontsize(fs)
    for (r,c),cell in t.get_celld().items():
        cell.set_edgecolor('white'); cell.set_facecolor('#163b4b' if r==0 else ('#eff4f5' if r%2 else '#fafbfc'))
        if r==0:cell.set_text_props(color='white',weight='bold')
    save(f)
def numfmt(x):
    return f'{x:,.1f}'.replace(',',' ').replace('.',',') if isinstance(x,(float,np.floating)) else str(x)
def metrics(j):
    a=data[j]; f=framify(a); ffull=f[f.scene!='solitalian_mare'];g=games[j];n=len(a)
    t=tim[tim.versione==['prima','dopo'][j]]
    e=t[t.misura=='motore'].us/1000;v=t[t.misura=='lettura'].us/1000
    neg=read('misti-'+['prima','finale'][j]);neg=neg[neg.truth==-1]
    return [
    ('Identità esatte','seme e valore corretti / ruoli attesi, %',100*a.ok.mean()),
    ('Copertura dei ruoli','ruoli con un candidato / attesi, %',100*a.found.mean()),
    ('Precisione tra le proposte','identità esatte / ruoli con candidato, %',100*a.ok.sum()/a.found.sum()),
    ('Valore esatto','valore corretto / ruoli attesi, %',100*a['rank'].mean()),
    ('Seme esatto','seme corretto / ruoli attesi, %',100*a.suit.mean()),
    ('Copertura automatica','margine ≥ 1,4 / ruoli attesi, %',100*a.automatic.mean()),
    ('Corrette automatiche','corrette con margine ≥ 1,4 / attese, %',100*a.safe.mean()),
    ('Precisione automatica','corrette automatiche / automatiche, %',100*a.safe.sum()/a.automatic.sum()),
    ('False certezze','errate con margine ≥ 1,4, conteggio',a.bad.sum()),
    ('Richieste potenziali di verifica','candidati sotto soglia, conteggio',(a.found&(a.automatic==0)).sum()),
    ('Carte mancate','ruoli privi di candidato, conteggio',(~a.found).sum()),
    ('Identità errate','proposte sbagliate, conteggio',(a.found&~a.ok).sum()),
    ('Schermate corrette su tutti i ruoli','ruoli annotati tutti corretti, %',100*f.esatta.mean()),
    ('Schermate interamente certe','ruoli annotati tutti corretti automatici, %',100*f.certe.mean()),
    ('Ruoli inattesi su tavoli con carte','conteggio, solo scene annotate integralmente',ffull.extra.sum()),
    ('Falsi positivi su tavoli vuoti','ruoli inattesi su 48 negativi',neg.extra_slots_in_frame.sum()),
    ('Schermate vuote con falso positivo','schermate con almeno un falso / 48, %',100*(neg.extra_slots_in_frame>0).mean()),
    ('Tempo medio di lettura','ms per read_briscola, sei condizioni ripetute',v.mean()),
    ('Lettura mediana','ms, sei condizioni ripetute',v.median()),
    ('Lettura P95','ms, sei condizioni ripetute',v.quantile(.95)),
    ('Tempo medio per consiglio','ms di calcolo motore con scelta, 1.000 campioni',e.mean()),
    ('Tempo mediano per consiglio','ms di calcolo motore',e.median()),
    ('Tempo P95 per consiglio','ms di calcolo motore',e.quantile(.95)),
    ('Tempo massimo per consiglio','ms di calcolo motore',e.max()),
    ('Giocate storiche corrette','eventi con carta e giocatore corretti / 1.760, %',100*g.correct.sum()/1760),
    ('Giocate storiche identificate corrette','corrette e marcate identificate / 1.760, %',100*g.correct_identified.sum()/1760),
    ('Giocate storiche mancanti','registrazioni assenti, conteggio',g.missing.sum()),
    ('Giocate storiche errate','registrazioni errate, conteggio',g.wrong.sum()),
    ('False identificazioni storiche','errate marcate identificate, conteggio',g.wrong_identified.sum()),
    ('Eventi duplicati','eventi di giocata duplicati, conteggio',g.duplicate_events.sum()),
    ('Partite ricostruite integralmente','40 giocate esatte, conteggio su 44',(g.correct==40).sum()),
    ('Chiusura alla ventesima presa','20 prese registrate, conteggio su 44',(g.tricks==20).sum()),
    ('Punteggi finali esatti','entrambi i punteggi esatti, partite su 44',(g.point_error==0).sum()),
    ('Errore assoluto nei punti','media della somma errori dei due giocatori',g.point_error.mean()),
    ('Turni pronti con stato corretto','turni pronti e stato esatto / 880, %',100*g.ready_with_correct_position.sum()/880),
    ('Turni pronti con stato errato','turni pronti ma stato errato / 880, %',100*g.ready_with_wrong_position.sum()/880),
    ('Turni errati segnalati come parziali','conteggio sui turni pronti con stato errato',g.wrong_position_with_partial_notice.sum()),
    ('Turni bloccati per fase','conteggio su 880',g.blocked_wrong_phase.sum()),
    ('Turni bloccati per conferma','conteggio su 880',g.blocked_confirmation.sum()),
    ('Turni bloccati per cattura','conteggio su 880',g.blocked_screen.sum()),
    ('Peggior sequenza senza stato pronto corretto','turni consecutivi nella peggiore partita',g.longest_unavailable_turns.max()),
    ('Letture senza tavolo','frame di replay senza scena',g.empty_reads.sum()),
    ('Errori di cattura iniettati','frame senza immagine nel replay',g.capture_errors.sum())]
met=[metrics(j) for j in range(2)]
metricrows=[[a[0],a[1],numfmt(a[2]),numfmt(b[2])] for a,b in zip(*met)]
pd.DataFrame([(a[0],a[1],a[2],b[2]) for a,b in zip(*met)],columns=['metrica','definizione','prima','dopo']).to_csv(out/'indicatori.csv',index=False)
plotdata=[]
for j,table in enumerate(data):
 for (k,v),g in table.groupby(['factor','level']):
    plotdata.append({'versione':titles[j],'factor':k,'level':v,'attese':len(g),'corrette':g.ok.sum(),'corrette_auto':g.safe.sum(),'errate_auto':g.bad.sum(),'us_medio_frame':framify(g).ms.mean()*1000})
samples=pd.DataFrame(plotdata);samples.to_csv(out/'curve.csv',index=False)
def plots(only=False,metric='corrette'):
    order=list(factors)
    for i in range(0,len(order),8):
        f=page({'corrette':'Identità esatte','corrette_auto':'Identità esatte sopra soglia','errate_auto':'False certezze'}[metric]+(' · riferimento' if only else ' · confronto'),'Asse verticale in percentuale di ruoli annotati. Ogni punto è una misura. Le linee uniscono soltanto i livelli provati.')
        axs=f.subplots(2,4);f.subplots_adjust(left=.075,right=.966,top=.805,bottom=.135,wspace=.34,hspace=.64)
        for ax,k in zip(axs.flat,order[i:i+8]):
            for j in range(1 if only else 2):
                g=samples[(samples.versione==titles[j])&(samples.factor==k)].sort_values('level')
                ax.plot(g.level,g[metric]/g.attese*100,marker='o' if not j else 'x',ls='--' if not j else '-',ms=5,color=colors[j],lw=1.5)
            ax.set_title(factors[k][0],fontsize=10,weight='bold');ax.set_ylim(-4,106);ax.set_yticks([0,50,100]);ax.grid(axis='y',alpha=.24);ax.set_xlabel(factors[k][1],fontsize=9)
            levels=g.level.to_list();ax.set_xticks(levels);ax.set_xticklabels(categories.get(k,[f'{v:g}' for v in levels]),fontsize=8,rotation=22 if k in categories else 0)
            ax.tick_params(axis='y',labelsize=8)
        f.legend([Line2D([],[],color=colors[0],ls='--',marker='o'),Line2D([],[],color=colors[1],marker='x')][:1 if only else 2],titles[:1 if only else 2],loc='lower center',bbox_to_anchor=(.5,.045),ncol=2,frameon=False)
        save(f)
def gamepage():
    rows=[]
    for k,a in games[0].groupby('scenario'):
        b=games[1][games[1].scenario==k]
        rows.append([k]+[f'{a[m].sum()} → {b[m].sum()}' for m in ['correct','wrong_identified','ready_with_correct_position','ready_with_wrong_position']])
    tab('Partite intere e cambi finestra','Quattro mazzi per scenario, 160 giocate e 80 turni. I turni pronti misurano lo stato disponibile prima del calcolo.', ['Scenario del replay','Giocate esatte','False certezze','Turni pronti esatti','Turni pronti errati'],rows,[.26,.18,.18,.19,.19],9)
def total(x):return f'{x.ok.sum()}/{len(x)}'
def build():
    global pdf,num
    num=0
    with PdfPages(out/'BrisMaster-audit.pdf') as pdf:
        narrative('Quanto regge il riconoscimento della Briscola?','Confronto controllato tra il codice pubblicato 0.6.13 (3b14734) e la candidata di questo audit.',[
        ('Il risultato sul banco',f'Nei 32 fattori singoli le identità esatte passano da {total(data[0])} a {total(data[1])}. Nei quattro stress combinati passano da {total(mix[0])} a {total(mix[1])}. Ogni numero conta la carta nel suo ruolo preciso.'),
        ('Dal pixel alla partita','Il banco isola prima gli errori della singola lettura; poi verifica l’accumulo degli errori su 44 partite intere, con 1.760 giocate e 880 turni. Lo stato esatto richiede mano, carta sul tavolo, briscola e insieme delle carte uscite corretti.'),
        ('Cosa è cambiato','Recupero del contrasto e dei canali attenuati, correzione delle proporzioni quando almeno due sagome concordano, eliminazione dei pannelli che ripetono esattamente lo stesso disegno. Il pannello del consiglio permette di aprire lo storico parziale e sospende le stime numeriche basate su punti incerti.'),
        ('Dove finisce la prova','Tre schermate reali annotate o parzialmente annotate e tavoli generati. Le prove temporali usano partite sintetiche. Le percentuali descrivono questi casi pesati secondo la griglia. Un sito mai osservato o una mossa completamente coperta restano fuori da una garanzia universale.')])
        for i in range(0,len(metricrows),14):
            tab('Indicatori misurati',f'Dal numero {i+1} al {min(i+14,len(metricrows))} di {len(metricrows)}. Le percentuali dei replay statici usano 1.896 ruoli sui 32 fattori.', ['Indicatore','Definizione o unità','0.6.13','Candidata'],metricrows[i:i+14],[.36,.43,.105,.105],8.5)
        rows=[]
        for k,(label,u) in factors.items():
            g=samples[(samples.factor==k)&(samples.versione==titles[1])].sort_values('level')
            b=samples[(samples.factor==k)&(samples.versione==titles[0])].sort_values('level')
            def span(t):
                pct=100*t.corrette/t.attese
                return f'{pct.min():.0f}–{pct.max():.0f}%'
            rows.append([label,u,', '.join(categories.get(k,[f'{v:g}' for v in g.level])),span(b),span(g)])
        for i in range(0,len(rows),16):
            tab('Variabili del sito e delle carte','Estremi osservati di accuratezza sulla griglia. Un intervallo 100–100% vale per i livelli e le scene indicati.', ['Fattore','Unità','Livelli provati','Prima min–max','Dopo min–max'],rows[i:i+16],[.235,.13,.345,.145,.145],8.7)
        for metric in ['corrette','corrette_auto','errate_auto']:plots(metric=metric)
        gamepage()
        mixed=[]
        for k,g in mix[0].groupby('factor'):
            h=mix[1][mix[1].factor==k]
            levels=lambda d:'  ·  '.join(str(int(a.ok.sum())) for _,a in d.groupby('level'))
            mixed.append([k,levels(g),levels(h)])
        tab('Stress combinati','Dal livello 0 al 4 aumenta la severità. Ogni cella contiene i riconoscimenti giusti su nove carte per livello.', ['Combinazione','0.6.13 · 5 livelli','Candidata · 5 livelli'],mixed,[.32,.34,.34])
        narrative('Quali interventi restano aperti?','Le modifiche conservate sono state rivalidate sugli stessi input. Gli esperimenti scartati sono riproducibili dai CSV.',[
        ('Rotazione, taglio e parti fuori campo','Raddrizzare solo le proporzioni ha recuperato carte deformate. Per rotazione o inclinazione estrema servono quadrilateri stabili prima della classificazione. Il tentativo di aumentare la nitidezza ha anche perso carte corrette. Per i ritagli il riferimento conserva tutti i ruoli originali e penalizza anche carte divenute invisibili.'),
        ('Tinta, dettaglio, illuminazione','La conversione in grigio aiuta sulle tinte estreme, ma ha perso identità e prodotto nuove false certezze in altre condizioni. Un filtro mediano e un aumento di nitidezza non hanno dato un vantaggio uniforme. Servono conferme temporali indipendenti prima di adottarli.'),
        ('Forma delle figure e tallone','Le curve di altezza, proporzioni e sovrapposizione mostrano dove mancano i dettagli della briscola o si fondono le sagome. Le prove a 32 px e con l’85% coperto danno priorità alla ricerca su fotogrammi precedenti e sulle viste parziali dello stesso mazzo.'),
        ('Perdita di cattura','Resta possibile ricevere una mossa su una posizione parziale. L’avviso con accesso alle correzioni e la soppressione delle stime numeriche rendono visibile questa condizione. I test distinguono la disponibilità di un consiglio dalla correttezza del suo stato.')])
        narrative('Protocollo e limiti','Tutti i file CSV conservano le decisioni elementari e le misure. Gli script e gli identificativi permettono di ripetere il confronto.',[
        ('Immagini e annotazioni','Per 24 fattori sono state trasformate separatamente due schermate del repository (cinque e quattro ruoli annotati). Una terza schermata Solitalian con mare è stata mantenuta per verifica separata, con le tre carte della mano annotate. La briscola semicoperta di questa scena è esclusa dal punteggio e dai conteggi di ruoli inattesi.'),
        ('Carte generate e distribuzioni','Otto fattori specifici delle carte sono stati provati su due disposizioni da sei ruoli con identità conosciute per costruzione. Il mazzo Después è disponibile solo per il seme di bastoni. Ogni replay statico apre un processo con memoria vuota. I risultati per carta sono correlati all’interno di uno stesso screenshot.'),
        ('Tempi','La misura per mossa copre il calcolo della scelta con almeno due carte legali, configurazione a 1.000 campioni e registro avversario vuoto. Il tempo di lettura misura read_briscola. La tabella tempi usa ripetizioni alternate; i tempi grezzi della scansione completa includono concorrenza e avvio a freddo del lettore.'),
        ('Ambiente e copertura','Replay CPU Linux del motore Rust utilizzato dall’app Windows. I tempi escludono cattura Windows, coda, disegno del pannello e reazione umana. La frequenza dei fotogrammi del sito, il ritardo della rete e le probabilità di vittoria sul sito reale richiedono sessioni reali con orari e verità osservabile.')])
    num=0
    with PdfPages(out/'Curve-riferimento-0.6.13.pdf') as pdf:plots(only=True)
build()
