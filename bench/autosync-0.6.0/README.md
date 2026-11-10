# Sincronizzazione per zone — base v0.6.0

Codice verificato: `e584d1ca30104aa2cc4710ac217ed9c4d0b79129`.
Ambiente: Linux x86_64, Rust 1.99.1. Comando: `bash scripts/validate.sh`.
Il log integrale e i conteggi sono conservati accanto a questo documento.

Le prove coprono i sei adattatori, transazioni e annullamento, molteplicità dei
mazzi, zone obbligatorie, persistenza, tempi originali di cattura e sospensione
al cambio di monitor. La catena del lettore verso il Blackjack è verificata anche
senza clic sulle singole carte.

Prova GUI su tavolo sintetico: K di cuori al banco, A e 9 di cuori nella mano.
Dopo aver disegnato le due zone e attivato la sincronizzazione, l’app ha inserito
le carte e mostrato «Stai». Dopo chiusura normale e riavvio, le zone erano
conservate e la riattivazione ha prodotto lo stesso risultato. Immagine sorgente
e schermata originali sono incluse. Questa prova GUI precede le ultime due
protezioni, coperte da test automatici e dalla build finale.

Non misura accuratezza su siti reali, forza dei motori, cattura Windows,
trasparenza o click-through nativi. Puntate, azioni degli avversari e mani divise
restano assistite. Per i benchmark dei motori vedere `../quality-0.6.0/`.
