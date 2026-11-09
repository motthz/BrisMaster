# Psy Fly: lettura affidabile e animazioni

Psy Fly indica le mosse di Briscola e Scopa sopra il tavolo del sito. Il sito riceve
sempre i clic dell'utente: la mosca non gioca automaticamente.

## Comportamento

- Nuovi gesti: saluto con le zampette, stiracchiamento, pulizia, ricerca con occhi e
  antenne, inchino sulla carta e danza di vittoria.
- Il battito delle ali varia continuamente con la velocità; il corpo si inclina in
  curva e i gesti entrano ed escono gradualmente. Le evidenziazioni restano leggibili
  anche durante le animazioni.
- Il fumetto distingue il motore che sta calcolando dalla ricerca delle carte.
- Una presa della Scopa è indicata solo quando tutte le sue carte sono localizzate.
- Le indicazioni scompaiono su dati scaduti, cattura guasta o coordinate di un'altra
  schermata. Gli esiti finali sono disponibili anche per la Scopa.
- I contorni usano direttamente l'ultima posizione misurata, senza interpolazione
  che li faccia restare indietro. Solo la mosca conserva inerzia e movimento morbido. Una carta che ricompare dopo un'occultazione non viene
  annunciata di nuovo.
- I cuoricini dopo una giocata richiedono che la ricostruzione abbia registrato la
  carta consigliata. Un semplice clic non viene descritto come una giocata riuscita.
- Spegnendo Psy Fly si ferma anche la lettura. Cambiando gioco il lettore inattivo
  viene sospeso; la preferenza di lettura è conservata per il ritorno al gioco.

## Tracking delle carte in movimento

La cattura punta a un intervallo di 16 ms, compatibilmente con il tempo richiesto
dal monitor e dal sistema. Un tracker separato segue ciascuna carta, comprese
quelle da prendere nella Scopa, a risoluzione piena fra i riconoscimenti completi.
La ricerca vicino all'ultima posizione ha precisione al pixel e gestisce piccoli
ridimensionamenti. Le carte possono muoversi indipendentemente; se una non viene
ritrovata il suo contorno viene nascosto. Le altre continuano a essere seguite.
Un risultato di riconoscimento in ritardo viene riallineato all'ultima cattura,
senza riportare indietro i bordi. Le catture più vecchie non sovrascrivono le nuove.

Il bordo sottile aderisce al rettangolo rilevato: pulsa il bagliore, non la sua
geometria. La simulazione della mosca usa passi massimi di 1/120 s e una molla
risolta analiticamente; ali antialias, sguardo e virate variano continuamente.

La precisione rimane vincolata alle immagini disponibili. Non è un aggancio al
DOM del sito: rotazioni, deformazioni forti, occlusioni o movimenti troppo rapidi
possono richiedere un nuovo riconoscimento. La latenza della cattura non può essere
azzerata; 16 ms è un obiettivo, non una garanzia di sincronizzazione con il monitor.

## Confini fra componenti

`Reader` conserva l'ordine degli eventi riconosciuti su un canale limitato a 16
risultati. Entrambi gli assistenti applicano ogni risultato prima di aggiornare
la scena mostrata. La coda delle catture rimane limitata a otto immagini e applica
la selezione esistente degli stati di passaggio quando il riconoscimento è lento:
non è una registrazione senza perdita di ogni fotogramma dello schermo.

Ogni scena ha un identificativo. `Placement` legge monitor, ritaglio, tracking,
salute della cattura e freschezza sotto lo stesso lock e produce uno snapshot
immutabile. Le coordinate sono utilizzabili solo per la scena corrispondente e
per due secondi dalla cattura/ultimo tracking valido. I risultati vecchi possono
ancora aggiornare la cronologia, ma non riattivano indicazioni scadute.

Il consiglio ha un'identità semantica (carta e presa) e un contesto (gioco e
revisione della partita). La persistenza temporanea non attraversa un cambio di
contesto e scade secondo un orologio reale, indipendente dal frame rate.

Per assistente resta al massimo un calcolo in corso: un cambio di posizione ne
invalida il risultato e il lavoro successivo attende che il precedente termini.
Non viene interrotto a metà il motore numerico.

La memoria delle carte viene salvata tramite file temporaneo e sostituzione
atomica. Gli errori sono visibili nel pannello di lettura. La possibilità di
apprendere esempi etichettati male resta una proprietà del riconoscimento:
la correzione/dimenticanza degli esempi è ancora necessaria in quei casi.

Su Windows le API di posizionamento ed esclusione dalle catture sono controllate
e ritentate. Se falliscono, l'overlay non viene disegnato e il pannello mostra
il problema. Il titolo include il processo per distinguere più istanze.

## Verifiche automatiche

```sh
cargo test --release -p brismaster
```

I test coprono eventi intermedi, errori di cattura senza nuovi frame, snapshot
con versione diversa o scaduta, coordinate negative, prese incomplete, esiti
Scopa senza tavolo visibile, risultati obsoleti del motore, salvataggio atomico,
cambi di carta, occultazione/ripristino, mancata festa senza giocata confermata,
limiti delle particelle e geometria finita durante tutti i nuovi gesti.

Per esportare l'anteprima dal disegno reale egui:

```sh
BRISMASTER_PSY_PREVIEW=/tmp/psy-preview cargo test -p brismaster export_animation_preview -- --ignored
python scripts/render-psyfly-preview.py /tmp/psy-preview /tmp/psy-render
```

Il renderer richiede NumPy e Pillow. Produce una GIF e una tavola PNG. È
un'anteprima software della grafica, non una prova delle finestre native.

## Verifiche manuali Windows

Provare Briscola e Scopa a 100%, 125%, 150% e 200% DPI, con due monitor anche di
scala diversa e origine negativa. Spostare il browser, cambiare monitor e area,
coprire/scoprire la mano e interrompere/ripristinare la cattura. Controllare
l'allineamento, il passaggio dei clic alle carte, la barra delle applicazioni e
l'assenza della mosca nelle immagini catturate. Verificare stop e ripresa quando
si spegne l'interruttore o si cambia gioco.
