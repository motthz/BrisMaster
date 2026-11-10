# Tempi dei kernel sulla base 0.6.0

Confronto sullo stesso ambiente Linux, Rust 1.99.1, profilo release del repository.
Baseline: archivio del tag `v0.6.0` (`c1e83b751d87b02e83dcbb7921b85ec2afdea372`),
con gli stessi quattro example di benchmark copiati senza altre modifiche.
Sette ripetizioni per versione, ordine baseline/modificata alternato, senza
compilazioni o test concorrenti durante la misura. Tempi interni degli eseguibili,
esclusi avvio di Cargo e compilazione. I dati grezzi sono in `kernel-results.json`.

| Carico fisso | 0.6.0, mediana ms | Modificata, mediana ms | Rapporto |
|---|---:|---:|---:|
| Hold'em, equità esatta su 200 river | 6,089 | 3,872 | 1,57× |
| Hold'em, funzione di equità usata dall'app su 200 river | 57,000 | 3,755 | 15,18× |
| Poker italiano, analisi di 200 mani e tutte le maschere | 121,206 | 116,489 | 1,04× |
| Blackjack, 10.000 interrogazioni EV con cache calda | 2,370 | 1,634 | 1,45× |
| Scala 40, 200 aperture | 0,993 | 1,008 | 0,99× |

Il rapporto è tempo baseline / tempo modificata. Le misure molto brevi sono
sensibili al rumore: Scala 40 non mostra un guadagno temporale; il cambiamento
riduce allocazioni. Il 4% del poker italiano va confermato su altri carichi.
Questi numeri non rappresentano la latenza dell'intera app, né la forza di gioco.

I checksum coincidono per tutti i percorsi esatti. La funzione dell'app Hold'em
passa da 4.000 campioni Monte Carlo a tutte le 990 combinazioni avversarie: il
checksum nuovo coincide con quello esatto, mentre quello vecchio varia per il
campionamento. Non è solo una diminuzione del numero di campioni.

Per ripetere: `bash scripts/bench-kernels.sh`. Gli example non modificano strategie,
reti allenate o registrazioni. Per valutare la forza servono i simulatori in
duplicato e gli intervalli di confidenza descritti nel README principale.
