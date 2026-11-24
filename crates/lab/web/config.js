// Impostazioni del sito. `invio`: indirizzo che riceve il CSV di ogni partita finita
// (POST, text/csv); "/api/partita" è il server Cloudflare in crates/lab/cloudflare.
// Vuoto = nessun invio: le partite restano nel browser e si scaricano.
window.LAB_CONFIG = {
  invio: '/api/partita',
  fonte: 'sito-laboratorio',
};
