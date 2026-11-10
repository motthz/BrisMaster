# Sincronizzazione automatica delle carte

Sulla base 0.6.0 il lettore comune può aggiornare automaticamente le carte degli
altri sei assistenti. Briscola e Scopa continuano a usare la propria ricostruzione
completa. La nuova modalità riconosce ingressi di carte, non clicca sul sito e non
inventa puntate, calate o azioni dell'avversario.

## Impostazione una volta per gioco

1. Apri il tavolo e attiva **Leggi carte dallo schermo**. Se serve, scegli l'area
   del sito con **Scegli area**, escludendo l'app e altri tavoli.
2. Nella finestra **Carte dallo schermo**, premi **Definisci zone del tavolo**.
   Scegli un ruolo e trascina sulla foto un rettangolo che comprenda tutta la sua
   fila di carte, anche gli spazi dove appariranno quelle successive.
3. Ripeti per gli altri ruoli necessari. Le zone devono essere separate. Quando
   cambi monitor o area di cattura, il lettore chiede di ridefinirle.
4. Attiva **Sincronizza automaticamente le carte nelle zone**. Servono almeno due
   immagini distinte e coerenti, distanti almeno 200 ms. Il tempo totale dipende
   anche da cattura e riconoscimento: non è una promessa di latenza di 200 ms.

Le zone sono salvate nelle impostazioni separatamente per ogni gioco. La modalità
si riattiva esplicitamente dopo un cambio di gioco o un nuovo avvio. Non servono
più conferme carta per carta quando il riconoscimento e lo stato sono coerenti.
Per un disegno nuovo usa ancora la scelta dell'etichetta e **Inserisci**: il lettore
apprende l'esempio confermato. Una carta incerta impedisce di usare quella lettura
per la sincronizzazione; il distacco mostrato non è una probabilità di correttezza.

| Gioco | Zone | Aggiornamenti automatici |
|---|---|---|
| Blackjack | Banco, La mia mano | Carta scoperta iniziale, due carte iniziali e una pescata alla volta; confronto per valore, non per seme. |
| Sette e mezzo | La mia coperta, La mia mano; Altre carte viste facoltativa | Coperta visibile al giocatore, carte ricevute e carte viste nel bersaglio selezionato. Le carte viste restano contate anche dopo che spariscono. |
| Scala 40 | La mia mano, Scarto in cima | Mano iniziale di 13/14 carte, carta del pozzo iniziale e nuova pescata. Il passaggio a 14 carte imposta il turno dopo la pescata. |
| Burraco | La mia mano, Scarto in cima | Le 11 carte iniziali, scarto iniziale, pescata dopo la scelta del tallone e carte del pozzetto nella fase corrispondente. |
| Poker italiano | La mia mano | Le cinque carte iniziali e la mano ricomposta dopo aver registrato il cambio. Solo carte dal 7 all'Asso. |
| Texas Hold'em | La mia mano, Board | Due carte iniziali, poi flop completo, turn e river quando le azioni del giro sono state inserite. |

Per Sette e mezzo seleziona nell'assistente **Altre carte viste** per raccogliere
quella zona; le carte proprie non vanno incluse. Per Scala 40 e Burraco le calate,
prese dal pozzo, scarti e azioni dell'avversario restano dichiarati nell'assistente.
Per i poker restano esplicite le puntate e il cambio. Una carta visibile non dice
se un avversario ha fatto check, call o raise.

## Continuità e correzioni

- Riordinare la mano non aggiunge carte. I doppi mazzi conservano le molteplicità;
  due jolly di disegno diverso valgono lo stesso tipo di carta.
- Il gruppo osservato è applicato interamente oppure non viene applicato. Se una
  carta confligge con quelle già inserite, il lettore ripristina lo stato precedente
  e sospende la modalità; correggi la situazione e riattivala.
- Una distribuzione incompleta non avvia la mano. Un cambio di fase incompatibile,
  una cattura scaduta, carte mancanti o copie impossibili non vengono "aggiustati"
  con ipotesi nascoste.
- **Annulla** non viene immediatamente contraddetto da una schermata già consumata.
  Un gruppo iniziale dell'Hold'em o del poker italiano ha una sola voce di annullamento.
- Una carta identica redistribuita nella stessa zona senza un cambiamento visivo
  distinguibile può richiedere una conferma manuale. La nuova mano viene avviata
  con il relativo comando dell'assistente, non cancellando automaticamente la
  partita precedente.
- Le mani divise del Blackjack richiedono ancora inserimento assistito: il lettore
  non mescola più mani in una sola zona.

Psy Fly usa la zona **La mia mano** anche per localizzare le carte. Se una nuova
osservazione non è ancora conciliata con il modello, attende carte coerenti e
azioni del turno anziché mostrare un consiglio vecchio su una carta nuova.
Le verifiche native Windows (DPI, più monitor, click-through ed esclusione dalle
catture) restano necessarie. Disegni diversi, ventagli e occlusioni richiedono prove
su tavoli reali: questa modalità non certifica tutti i siti.

La stabilità viene misurata sui tempi originali delle catture: una consegna lenta
all’interfaccia non rende stabile una sequenza troppo rapida. Se cambiano monitor
o scala durante la lettura, la sincronizzazione si disattiva: verifica le zone
e riattivala esplicitamente.
