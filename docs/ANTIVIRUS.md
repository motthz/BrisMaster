# Se l'antivirus blocca BrisMaster.exe

BrisMaster è un programma senza firma digitale, scaricato da internet e usato da poche persone: per Windows Defender e SmartScreen è il profilo tipico di un "falso positivo". Il codice è tutto in questo repository e l'exe viene compilato da GitHub Actions direttamente dal codice (vedi `.github/workflows/release.yml`); accanto all'exe ogni release pubblica `BrisMaster.exe.sha256` per controllare che il file scaricato sia quello compilato.

Dalla versione 0.4.3 l'exe contiene l'icona, i dati della versione (nome del prodotto, autore, descrizione, versione: si vedono in *tasto destro → Proprietà → Dettagli*) e un manifest che dichiara che il programma gira senza privilegi di amministratore. Sono i dati che hanno tutti i programmi normali e che riducono i blocchi; non eliminano del tutto gli avvisi finché l'exe non è firmato.

## SmartScreen: "Windows ha protetto il PC"

1. Clicca **Ulteriori informazioni**.
2. Clicca **Esegui comunque**.

Serve una volta sola per ogni versione.

## Windows Defender ha messo il file in quarantena

1. Apri **Sicurezza di Windows → Protezione da virus e minacce → Cronologia protezione**.
2. Apri l'evento di BrisMaster, scegli **Azioni → Consenti nel dispositivo** (o *Ripristina*).

## Esclusione per la cartella di BrisMaster

Così Defender non controlla più l'exe ad ogni aggiornamento automatico. Usa una cartella dedicata solo a BrisMaster, per esempio `Documenti\BrisMaster`.

1. **Sicurezza di Windows → Protezione da virus e minacce → Impostazioni di Protezione da virus e minacce → Gestisci impostazioni**.
2. Scorri fino a **Esclusioni → Aggiungi o rimuovi esclusioni**.
3. **Aggiungi un'esclusione → Cartella** e scegli la cartella di BrisMaster.

## Segnalare il falso positivo a Microsoft

Dalla pagina <https://www.microsoft.com/wdsi/filesubmission> si può inviare `BrisMaster.exe` come "software scorretto rilevato" (incorrectly detected). Di solito in qualche giorno Microsoft aggiorna le definizioni e il blocco sparisce per tutti.

## La soluzione definitiva: la firma del codice

Un exe firmato con un certificato di firma del codice (Authenticode) viene bloccato molto meno, e SmartScreen smette di avvisare man mano che il certificato si fa una reputazione. Il certificato è intestato a una persona o a un'azienda, va verificata l'identità e costa (servizi come Azure Trusted Signing o certificati commerciali, da qualche decina a qualche centinaio di euro l'anno). Se ne prendi uno, la firma si aggiunge al workflow di release in un passo, con il certificato salvato nei segreti del repository.
