// Tavolo del laboratorio: la pagina mostra solo quello che la persona può vedere;
// partita, arbitro e Massima stanno nel worker (lab.wasm, lo stesso codice del repo).
(function () {
  'use strict';
  const CFG = window.LAB_CONFIG || {};
  const VALORI = ['A', '2', '3', '4', '5', '6', '7', 'F', 'C', 'R'];
  const NOMI_VALORI = ['Asso', 'Due', 'Tre', 'Quattro', 'Cinque', 'Sei', 'Sette', 'Fante', 'Cavallo', 'Re'];
  const SEMI = ['Denari', 'Coppe', 'Spade', 'Bastoni'];
  const COLORI = ['var(--denari)', 'var(--coppe)', 'var(--spade)', 'var(--bastoni)'];
  const $ = (id) => document.getElementById(id);

  // Memoria locale: solo comodità (codice e copia delle partite). Se non c'è, si gioca lo stesso.
  const leggi = (k, d) => { try { const v = localStorage.getItem(k); return v == null ? d : JSON.parse(v); } catch (_) { return d; } };
  const salva = (k, v) => { try { localStorage.setItem(k, JSON.stringify(v)); } catch (_) { /* niente */ } };
  const casuale = (n) => { const a = new Uint32Array(n); crypto.getRandomValues(a); return a; };
  const esa = (n) => Array.from(casuale(n), (x) => x.toString(16).padStart(8, '0')).join('');

  let giocatore = leggi('lab_giocatore', null);
  if (!giocatore) { giocatore = 'p-' + esa(1); salva('lab_giocatore', giocatore); }
  const sessione = 's-' + new Date().toISOString().slice(0, 10) + '-' + esa(1);
  let partite = leggi('lab_partite', []); // [{id, csv, inviata}]
  let apre = casuale(1)[0] & 1;

  // Worker con richieste numerate.
  const worker = new Worker('worker.js');
  let prossimo = 1; const attese = new Map();
  worker.onmessage = (ev) => { const a = attese.get(ev.data.id); if (!a) return; attese.delete(ev.data.id); ev.data.errore ? a.no(new Error(ev.data.errore)) : a.si(ev.data); };
  const chiedi = (m) => new Promise((si, no) => { const id = prossimo++; attese.set(id, { si, no }); worker.postMessage({ id, ...m }); });
  const pausa = (ms) => new Promise((r) => setTimeout(r, ms));

  function carta(c, opz = {}) {
    const el = document.createElement(opz.bottone ? 'button' : 'div');
    el.className = 'carta' + (opz.piccola ? ' piccola' : '');
    if (c == null) { el.className += ' dorso'; el.setAttribute('aria-label', 'carta coperta'); return el; }
    const s = Math.floor(c / 10), v = c % 10;
    el.setAttribute('aria-label', `${NOMI_VALORI[v]} di ${SEMI[s]}`);
    const a = document.createElement('span'); a.className = 'v'; a.textContent = VALORI[v]; a.style.color = COLORI[s];
    const b = document.createElement('span'); b.className = 'simbolo'; b.style.background = COLORI[s];
    const d = document.createElement('span'); d.className = 's'; d.textContent = SEMI[s]; d.style.color = COLORI[s];
    el.append(a, b, d);
    return el;
  }

  let vista = null, inizioTurno = 0, occupato = false, banco = []; // banco: carte da mostrare al centro

  function disegna() {
    const v = vista;
    $('avv').replaceChildren(...Array.from({ length: v.carte_massima }, () => carta(null)));
    const centro = banco.length ? banco.map((c) => carta(c)) : (v.tavolo != null ? [carta(v.tavolo)] : []);
    if (!centro.length) { const x = document.createElement('div'); x.className = 'vuoto'; centro.push(x); }
    $('centro').replaceChildren(...centro);
    $('briscola').replaceChildren(v.briscola_pescata ? document.createTextNode('') : carta(v.briscola, { piccola: true }));
    $('mazzo').textContent = v.mazzo_rimasto ? `Mazzo: ${v.mazzo_rimasto}` : 'Mazzo finito';
    if (v.briscola_pescata) $('mazzo').textContent += ` · briscola: ${SEMI[Math.floor(v.briscola / 10)]}`;
    const mia = v.mano.map((c) => {
      const b = carta(c, { bottone: true });
      b.disabled = occupato || v.tocca !== 'umano' || banco.length > 0;
      b.addEventListener('click', () => giocaUmano(c));
      return b;
    });
    $('mia').replaceChildren(...mia);
  }

  function stato(t) { $('stato').textContent = t; }

  async function nuovaPartita() {
    const s = casuale(2);
    const id = 'g-' + esa(3);
    apre = 1 - apre;
    banco = []; occupato = false;
    const r = await chiedi({ tipo: 'nuova', semeAlto: s[0], semeBasso: s[1], umano: 0, apre, partita: id, giocatore, sessione, fonte: CFG.fonte || 'sito-laboratorio' });
    vista = r.vista; vista.id = id;
    $('fine').classList.add('nascosto'); $('tavolo').classList.remove('nascosto');
    await turno();
  }

  async function turno() {
    if (vista.tocca === 'fine') return finePartita();
    if (vista.tocca === 'umano') {
      disegna(); stato(vista.tavolo != null ? 'Tocca a te rispondere' : 'Tocca a te aprire');
      inizioTurno = performance.now();
      return;
    }
    occupato = true; disegna(); stato('Massima ci pensa…');
    const t0 = performance.now();
    const r = await chiedi({ tipo: 'massima' });
    await pausa(Math.max(0, 700 - (performance.now() - t0)));
    await dopoGiocata(r.carta, r.vista);
  }

  async function giocaUmano(c) {
    if (occupato || vista.tocca !== 'umano' || banco.length) return;
    occupato = true;
    const tempo = Math.round(performance.now() - inizioTurno);
    try {
      const r = await chiedi({ tipo: 'gioca', carta: c, tempo });
      await dopoGiocata(c, r.vista);
    } catch (e) { occupato = false; stato('Errore: ' + e.message); disegna(); }
  }

  // Mostra la presa chiusa per un momento prima di raccoglierla.
  async function dopoGiocata(c, nuova) {
    const chiusa = vista.tavolo != null;
    const vecchia = vista; vista = Object.assign(nuova, { id: vecchia.id });
    if (chiusa && nuova.ultima_presa) {
      const p = nuova.ultima_presa;
      banco = [p.prima, p.seconda];
      // Durante la pausa la mano mostrata è quella di prima della pescata.
      const mano = vecchia.mano.filter((x) => x !== c);
      const mostra = Object.assign({}, vecchia, { mano, tavolo: null, tocca: 'attesa', carte_massima: vecchia.carte_massima - (vecchia.tocca === 'massima' ? 1 : 0) });
      const salvaVista = vista; vista = mostra; disegna(); vista = salvaVista;
      stato(p.vince === 'umano' ? `Prendi tu (${p.punti} punti)` : `Prende Massima (${p.punti} punti)`);
      await pausa(1400);
      banco = [];
    }
    occupato = false;
    await turno();
  }

  async function finePartita() {
    disegna(); stato('Partita finita');
    const { csv } = await chiedi({ tipo: 'csv', intestazione: false });
    const voce = { id: vista.id, csv, inviata: false };
    partite.push(voce); salva('lab_partite', partite);
    $('contatore').textContent = partite.length;
    const u = vista.punti_umano, m = vista.punti_massima;
    $('esito').textContent = u > 60 ? 'Hai vinto!' : u === 60 ? 'Pareggio' : 'Ha vinto Massima';
    $('punti').textContent = `Tu ${u} punti, Massima ${m}.`;
    $('fine').classList.remove('nascosto');
    await invia(voce);
  }

  const INTESTAZIONE = 'schema,partita,giocatore,sessione,regole,fonte,origine,mossa,presa,posto,apre,briscola,mano,tavolo,mazzo_rimasto,punti_miei,punti_avversario,legali,scelta,storia,consigli_visibili,tempo_ms,qualita,mazzo,motore';

  async function invia(voce) {
    if (!CFG.invio) { $('invio').textContent = 'Partita salvata in questo browser. Quando vuoi, scarica il file e mandalo a chi ti ha invitato.'; return; }
    $('invio').textContent = 'Invio della partita…';
    try {
      const r = await fetch(CFG.invio, { method: 'POST', headers: { 'Content-Type': 'text/csv' }, body: INTESTAZIONE + '\n' + voce.csv });
      if (!r.ok) throw new Error('HTTP ' + r.status);
      voce.inviata = true; salva('lab_partite', partite);
      $('invio').textContent = 'Partita inviata. Grazie!';
    } catch (e) {
      $('invio').textContent = 'Invio non riuscito: la partita resta in questo browser e si può scaricare.';
    }
    // Riprova quelle rimaste indietro.
    for (const p of partite.filter((x) => !x.inviata && x !== voce)) {
      try { const r = await fetch(CFG.invio, { method: 'POST', headers: { 'Content-Type': 'text/csv' }, body: INTESTAZIONE + '\n' + p.csv }); if (r.ok) { p.inviata = true; salva('lab_partite', partite); } } catch (_) { break; }
    }
  }

  function scarica() {
    const testo = INTESTAZIONE + '\n' + partite.map((p) => p.csv).join('');
    const url = URL.createObjectURL(new Blob([testo], { type: 'text/csv' }));
    const a = document.createElement('a'); a.href = url; a.download = `partite-${giocatore}.csv`;
    document.body.append(a); a.click(); a.remove(); setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  $('codice').textContent = giocatore;
  $('contatore').textContent = partite.length;
  $('consenso').addEventListener('change', () => { $('inizia').disabled = !$('consenso').checked || !pronto; });
  $('inizia').addEventListener('click', () => { $('intro').classList.add('nascosto'); nuovaPartita().catch((e) => stato('Errore: ' + e.message)); });
  $('ancora').addEventListener('click', () => nuovaPartita().catch((e) => stato('Errore: ' + e.message)));
  $('scarica').addEventListener('click', scarica);
  let pronto = false;
  chiedi({ tipo: 'csv' }).then(() => { pronto = true; $('caricamento').textContent = 'Motore pronto.'; $('inizia').disabled = !$('consenso').checked; })
    .catch((e) => { $('caricamento').textContent = 'Il motore non si carica: ' + e.message; });
})();
