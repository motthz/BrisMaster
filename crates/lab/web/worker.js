// Massima gira qui, fuori dalla pagina, così il tavolo non si blocca mentre pensa.
let e = null;
const dec = new TextDecoder(), enc = new TextEncoder();
const out = () => dec.decode(new Uint8Array(e.memory.buffer, e.lab_out_ptr(), e.lab_out_len()));
const put = (s) => {
  const b = enc.encode(s);
  const p = e.lab_alloc(b.length);
  new Uint8Array(e.memory.buffer, p, b.length).set(b);
  return [p, b.length];
};
const pronto = fetch('lab.wasm')
  .then((r) => r.arrayBuffer())
  .then((b) => WebAssembly.instantiate(b, {}))
  .then((r) => { e = r.instance.exports; });

onmessage = async (ev) => {
  const m = ev.data;
  try {
    await pronto;
    let risposta;
    if (m.tipo === 'nuova') {
      const [p, l] = put([m.partita, m.giocatore, m.sessione, m.fonte].join(','));
      e.lab_nuova(m.semeAlto >>> 0, m.semeBasso >>> 0, m.umano, m.apre, p, l);
      risposta = { vista: JSON.parse(out()) };
    } else if (m.tipo === 'gioca') {
      if (e.lab_gioca(m.carta, m.tempo) !== 0) throw new Error(out());
      e.lab_vista();
      risposta = { vista: JSON.parse(out()) };
    } else if (m.tipo === 'massima') {
      const c = e.lab_massima();
      if (c < 0) throw new Error(out());
      e.lab_vista();
      risposta = { carta: c, vista: JSON.parse(out()) };
    } else if (m.tipo === 'csv') {
      e.lab_csv(m.intestazione ? 1 : 0);
      risposta = { csv: out() };
    }
    postMessage({ id: m.id, ...risposta });
  } catch (err) {
    postMessage({ id: m.id, errore: String(err && err.message || err) });
  }
};
