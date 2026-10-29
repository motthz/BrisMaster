# Se l'antivirus blocca BrisMaster.exe

BrisMaster è un programma senza firma digitale, scaricato da internet e usato da poche persone: per Windows Defender e SmartScreen è il profilo tipico di un "falso positivo". Il codice è tutto in questo repository e l'exe viene compilato da GitHub Actions direttamente dal codice (vedi `.github/workflows/release.yml`); accanto all'exe ogni release pubblica `BrisMaster.exe.sha256` per controllare che il file scaricato sia quello compilato.

Dalla versione 0.4.3 l'exe contiene l'icona, i dati della versione (nome del prodotto, autore, descrizione, versione: si vedono in *tasto destro → Proprietà → Dettagli*) e un manifest che dichiara che il programma gira senza privilegi di amministratore. Sono i dati che hanno tutti i programmi normali e che riducono i blocchi; non eliminano del tutto gli avvisi finché l'exe non è firmato.

## Perché succede

Gli antivirus giudicano i programmi anche dalla "reputazione": un exe senza firma digitale, appena compilato (ogni versione ha un'impronta nuova) e scaricato da poche persone viene trattato con sospetto. In più BrisMaster fa cose che, prese da sole, somigliano a quelle dei programmi malevoli: fotografa lo schermo (per leggere la partita) e scarica da internet una nuova versione di sé stesso per aggiornarsi. Per questo il blocco può tornare a ogni versione nuova finché l'exe non è firmato.

Per capire quale regola scatta serve il nome della minaccia che l'antivirus mostra (per esempio `Trojan:Win32/Wacatac.B!ml` in Windows Defender: il `!ml` vuol dire che è un giudizio "automatico", non il riconoscimento di un virus vero).

## Con l'installatore

Dalla versione 0.4.6 c'è `BrisMaster-Setup.exe`, che installa BrisMaster sempre nella stessa cartella: `%LOCALAPPDATA%\Programs\BrisMaster` (incolla questo percorso nella barra degli indirizzi di Esplora file per aprirla). Una sola esclusione per quella cartella (vedi sotto) vale anche per tutti gli aggiornamenti automatici.

## Bitdefender

Bitdefender blocca BrisMaster soprattutto con **Advanced Threat Defense**, che giudica i programmi da quello che fanno mentre sono in esecuzione: BrisMaster fotografa lo schermo più volte al secondo (per leggere la partita) e, fino alla versione 0.5.1, si aggiornava sostituendo il proprio exe mentre era aperto. Dalla 0.5.2 l'aggiornamento passa dall'installatore, come per i programmi comuni; la lettura dello schermo invece resta, perché è quello che fa l'app.

1. Installa BrisMaster con `BrisMaster-Setup.exe`: l'exe sta sempre in `%LOCALAPPDATA%\Programs\BrisMaster\BrisMaster.exe`, anche dopo gli aggiornamenti.
2. Se è in quarantena: **Protezione → Antivirus → Quarantena**, seleziona BrisMaster e **Ripristina**.
3. Eccezione: **Protezione → Advanced Threat Defense → Impostazioni → Gestisci eccezioni → + Aggiungi un'eccezione**, incolla il percorso `%LOCALAPPDATA%\Programs\BrisMaster\BrisMaster.exe` (o sfoglia fino all'exe), attiva **Advanced Threat Defense** e, se c'è, anche **Antivirus**, poi **Salva**. Le eccezioni di Advanced Threat Defense accettano solo file `.exe`, non cartelle: per questo conta che il percorso resti sempre lo stesso.
4. Segnala il falso positivo ai Bitdefender Labs dal modulo <https://www.bitdefender.com/consumer/support/answer/29358/> (se non funziona, da <https://www.bitdefender.com/en-us/business/submit>): categoria *False Positive*, allega `BrisMaster.exe` (massimo 25 MB), scrivi il nome della minaccia che compare nella notifica di Bitdefender e, se puoi, aggiungi uno screenshot. Rispondono per email dopo l'analisi, di solito in pochi giorni, e il blocco sparisce per tutti finché esce una versione nuova.

Guide ufficiali: [eccezioni di Advanced Threat Defense](https://www.bitdefender.com/consumer/support/answer/130706/), [app attendibile bloccata da Advanced Threat Defense](https://www.bitdefender.com/consumer/support/answer/88974).

## SmartScreen: "Windows ha protetto il PC"

1. Clicca **Ulteriori informazioni**.
2. Clicca **Esegui comunque**.

Serve una volta sola per ogni versione.

## Windows Defender ha messo il file in quarantena

1. Apri **Sicurezza di Windows → Protezione da virus e minacce → Cronologia protezione**.
2. Apri l'evento di BrisMaster, scegli **Azioni → Consenti nel dispositivo** (o *Ripristina*).

## Esclusione per la cartella di BrisMaster

Così Defender non controlla più l'exe ad ogni aggiornamento automatico. Usa la cartella dove è installato BrisMaster, `%LOCALAPPDATA%\Programs\BrisMaster` (o, se usi l'exe senza installatore, una cartella dedicata solo a lui).

1. **Sicurezza di Windows → Protezione da virus e minacce → Impostazioni di Protezione da virus e minacce → Gestisci impostazioni**.
2. Scorri fino a **Esclusioni → Aggiungi o rimuovi esclusioni**.
3. **Aggiungi un'esclusione → Cartella** e scegli la cartella di BrisMaster.

## Segnalare il falso positivo a Microsoft

Dalla pagina <https://www.microsoft.com/wdsi/filesubmission> si può inviare `BrisMaster.exe` come "software scorretto rilevato" (incorrectly detected). Di solito in qualche giorno Microsoft aggiorna le definizioni e il blocco sparisce per tutti.

## La soluzione definitiva: la firma del codice

Un exe firmato con un certificato di firma del codice (Authenticode) viene bloccato molto meno, e SmartScreen smette di avvisare man mano che il certificato si fa una reputazione. È l'unica soluzione che vale per tutti e per sempre; le altre (esclusione, segnalazione) vanno ripetute.

- **SignPath Foundation** firma gratis i progetti open source (BrisMaster lo è, licenza MIT): si fa domanda su <https://signpath.org>, e se il progetto viene accettato la firma si aggiunge al workflow di release con la loro GitHub Action.
- **Azure Trusted Signing** o un certificato commerciale: a pagamento (da qualche euro al mese a qualche centinaio l'anno), con verifica dell'identità di chi firma.

In tutti i casi la firma si aggiunge al workflow di release in un passo, con le credenziali salvate nei segreti del repository, e firma sia `BrisMaster.exe` sia l'installatore.
