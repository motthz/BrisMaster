// Server del laboratorio: riceve il CSV di ogni partita finita, lo controlla in modo leggero
// (il controllo completo è `lab-valida` nel repo) e lo salva con un codice al posto dell'IP.
//
//   POST /api/partita     corpo text/csv: intestazione + 40 righe human_decision_v1
//   GET  /api/conteggio   partite, giocatori e IP diversi (solo numeri, pubblico)
//   GET  /api/esporta?chiave=CHIAVE_EXPORT   tutte le partite in un unico CSV, con codice_ip

import { DurableObject } from 'cloudflare:workers';

const SCHEMA = 'human_decision_v1';
const MAX_BYTE = 64 * 1024;
const MAX_PARTITE_IP_GIORNO = 200;

async function codiceIp(ip, sale) {
  const k = await crypto.subtle.importKey('raw', new TextEncoder().encode(sale), { name: 'HMAC', hash: 'SHA-256' }, false, ['sign']);
  const f = new Uint8Array(await crypto.subtle.sign('HMAC', k, new TextEncoder().encode(ip)));
  return 'ip-' + Array.from(f.slice(0, 6), (b) => b.toString(16).padStart(2, '0')).join('');
}

// Controllo leggero: intestazione giusta, 40 righe della stessa partita, mosse 0..39.
export function controlla(testo) {
  const righe = testo.replace(/^﻿/, '').split(/\r?\n/).filter((l) => l.trim() !== '');
  if (righe.length !== 41) return { errore: `servono 41 righe (intestazione + 40), arrivate ${righe.length}` };
  const col = righe[0].split(',');
  const i = (n) => col.indexOf(n);
  for (const n of ['schema', 'partita', 'giocatore', 'mossa', 'origine']) if (i(n) < 0) return { errore: `manca la colonna ${n}` };
  const dati = righe.slice(1).map((l) => l.split(','));
  const partita = dati[0][i('partita')], giocatore = dati[0][i('giocatore')];
  if (!/^[\w-]{1,80}$/.test(partita) || !/^[\w-]{1,80}$/.test(giocatore)) return { errore: 'codici di partita o giocatore non validi' };
  const mosse = new Set();
  for (const d of dati) {
    if (d.length !== col.length) return { errore: 'righe con un numero di colonne diverso' };
    if (d[i('schema')] !== SCHEMA) return { errore: 'schema sconosciuto' };
    if (d[i('partita')] !== partita || d[i('giocatore')] !== giocatore) return { errore: 'righe di partite diverse' };
    mosse.add(+d[i('mossa')]);
  }
  for (let m = 0; m < 40; m++) if (!mosse.has(m)) return { errore: `manca la mossa ${m}` };
  return { partita, giocatore, intestazione: righe[0], righe: righe.slice(1) };
}

export class Archivio extends DurableObject {
  constructor(ctx, env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(`CREATE TABLE IF NOT EXISTS partite (
      partita TEXT PRIMARY KEY, giocatore TEXT, codice_ip TEXT, ricevuta TEXT, intestazione TEXT, righe TEXT)`);
  }
  salva(p, codice) {
    const oggi = new Date().toISOString().slice(0, 10);
    const n = this.sql.exec('SELECT COUNT(*) AS n FROM partite WHERE codice_ip = ? AND ricevuta >= ?', codice, oggi).one().n;
    if (n >= MAX_PARTITE_IP_GIORNO) return 'troppe partite oggi da questa rete';
    const c = this.sql.exec('INSERT OR IGNORE INTO partite VALUES (?, ?, ?, ?, ?, ?)', p.partita, p.giocatore, codice,
      new Date().toISOString(), p.intestazione, p.righe.join('\n'));
    return c.rowsWritten ? 'salvata' : 'già ricevuta';
  }
  conteggio() {
    return this.sql.exec('SELECT COUNT(*) AS partite, COUNT(DISTINCT giocatore) AS giocatori, COUNT(DISTINCT codice_ip) AS ip FROM partite').one();
  }
  esporta() {
    const tutte = this.sql.exec('SELECT codice_ip, intestazione, righe FROM partite ORDER BY ricevuta').toArray();
    if (!tutte.length) return '';
    let out = '', ultima = null;
    for (const p of tutte) {
      if (p.intestazione !== ultima) { out += 'codice_ip,' + p.intestazione + '\n'; ultima = p.intestazione; }
      out += p.righe.split('\n').map((r) => p.codice_ip + ',' + r).join('\n') + '\n';
    }
    return out;
  }
}

const json = (o, status = 200) => new Response(JSON.stringify(o), { status, headers: { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store' } });

export default {
  async fetch(req, env) {
    const url = new URL(req.url);
    const archivio = env.ARCHIVIO.get(env.ARCHIVIO.idFromName('archivio'));
    if (url.pathname === '/api/partita' && req.method === 'POST') {
      if (!env.SALE) return json({ errore: 'server non configurato (manca SALE)' }, 503);
      const corpo = await req.text();
      if (corpo.length > MAX_BYTE) return json({ errore: 'troppo grande' }, 413);
      const p = controlla(corpo);
      if (p.errore) return json({ errore: p.errore }, 400);
      const ip = req.headers.get('CF-Connecting-IP') || 'sconosciuto';
      const esito = await archivio.salva(p, await codiceIp(ip, env.SALE));
      return json({ esito }, esito.startsWith('troppe') ? 429 : 200);
    }
    if (url.pathname === '/api/conteggio' && req.method === 'GET') return json(await archivio.conteggio());
    if (url.pathname === '/api/esporta' && req.method === 'GET') {
      if (!env.CHIAVE_EXPORT || url.searchParams.get('chiave') !== env.CHIAVE_EXPORT) return json({ errore: 'chiave sbagliata' }, 403);
      return new Response(await archivio.esporta(), { headers: { 'content-type': 'text/csv; charset=utf-8', 'content-disposition': 'attachment; filename="partite-laboratorio.csv"', 'cache-control': 'no-store' } });
    }
    if (url.pathname.startsWith('/api/')) return json({ errore: 'non trovato' }, 404);
    return env.ASSETS.fetch(req);
  },
};
