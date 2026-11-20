## Riconoscimento della Briscola

BrisMaster isola il tavolo dai bordi scuri uniformi delle finestre compatte. Sui panni chiari con tinta uniforme usa la differenza di colore per separare lo sfondo dalla carta. La ricerca delle giocate al centro sfrutta la misura della mano anche con carte più grandi. Le coordinate dei riquadri tornano alla posizione originale sullo schermo.

Le correzioni della mano rispettano i punti dello storico privi di evento. Questo risolve un arresto riprodotto nella 0.6.12. Se mancano prese osservate, il pannello del consiglio segnala che carte o punti possono essere incerti.

## Confronto con la 0.6.12

Su quattro partite sintetiche con tavolo chiaro, scritte, bordi e carte da 80 px, la 0.6.13 ricostruisce 160 giocate su 160, senza false certezze. La 0.6.12 si arresta in una partita e totalizza 7 giocate corrette su 120 nelle altre tre, con 45 false certezze. Con finestra stabile entrambe ricostruiscono 160 giocate su 160.

Il banco riproduce anche la scomparsa della finestra e la perdita di fotogrammi durante una presa. Una carta rimasta invisibile può lasciare incompleto lo storico. I risultati riportati riguardano replay sintetici e la precisione sul sito reale dipende dalla cattura disponibile.
