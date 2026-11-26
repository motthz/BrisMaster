"""Analisi del banco `prima_lettura` (docs/SPERIMENTAZIONE-RICONOSCIMENTO.md).

    python3 scripts/prima-lettura.py CARTELLA_PARTITE LETTURE.csv [--finestra 10]

CARTELLA_PARTITE contiene una sottocartella per partita con `verita.json`; LETTURE.csv è il file
scritto dal test con PRIMA_CSV (partita, schermata, ruolo, carta se giusta o ?, carta letta,
distacco). Ruoli: 0 mano, 1 briscola, 2 tavolo. La riga con ruolo -1 attesta che
la schermata è stata analizzata, anche quando non ci sono letture.

Prima lettura di una carta: dalla schermata in cui la verità la mette in un ruolo, la prima
lettura *nuova* in quel ruolo (una carta letta che non era già nel ruolo nella verità della
schermata prima) entro `--finestra` schermate. La verità di alcune registrazioni anticipa
l'animazione di qualche schermata: per questo non si guarda solo la schermata stessa.

Conta: prima lettura giusta, giusta e sicura (distacco >= 1.4), sbagliata, mancata; e su tutte
le letture: identità sbagliate date per sicure (una carta che non è da nessuna parte sullo
schermo) e ruoli sbagliati (carta vera, ma in un altro ruolo).
"""
import csv, json, os, sys, collections

AUTO = 1.4
SHORT = {}


def short_names():
    suits = [('Denari', 'D'), ('Coppe', 'C'), ('Spade', 'S'), ('Bastoni', 'B')]
    ranks = [('Asso', 'A'), ('Due', '2'), ('Tre', '3'), ('Quattro', '4'), ('Cinque', '5'), ('Sei', '6'),
             ('Sette', '7'), ('Fante', 'F'), ('Cavallo', 'C'), ('Re', 'R')]
    for s, sc in suits:
        for r, rc in ranks:
            SHORT[f'{r} di {s}'] = rc + sc


def main():
    short_names()
    root, path = sys.argv[1], sys.argv[2]
    window = int(sys.argv[sys.argv.index('--finestra') + 1]) if '--finestra' in sys.argv else 10
    reads = collections.defaultdict(list)
    processed = collections.Counter()
    with open(path, newline='', encoding='utf-8') as csv_file:
        for game, frame, role, _, card, margin in csv.reader(csv_file):
            key = (game, frame)
            if role == '-1':
                processed[key] += 1
            else:
                reads[key].append((int(role), card, float(margin)))
    games = sorted(entry for entry in os.listdir(root)
                   if os.path.isfile(os.path.join(root, entry, 'verita.json')))
    manifests = {}
    expected = set()
    for game in games:
        with open(os.path.join(root, game, 'verita.json'), encoding='utf-8') as manifest:
            manifests[game] = json.load(manifest)
        for entry in manifests[game]:
            key = (game, entry['frame'])
            if key in expected:
                sys.exit(f"Fotogramma duplicato nelle annotazioni: {key}")
            expected.add(key)
    if not expected:
        sys.exit("Nessun fotogramma annotato nella cartella indicata.")
    invalid = {key for key in expected if processed[key] != 1}
    invalid.update((processed.keys() | reads.keys()) - expected)
    if invalid:
        game, frame = min(invalid)
        sys.exit(
            f"CSV incompleto o incoerente: {len(invalid)} fotogrammi, "
            f"primo: {game}/{frame}. Rigenerare PRIMA_CSV con il test aggiornato."
        )

    tot = collections.Counter()
    by_role = collections.defaultdict(collections.Counter)
    for g, log in manifests.items():
        truth = []
        for e in log:
            hand = [SHORT[c] for c in e.get('hand', [])]
            table = [SHORT[t['card']] for t in e.get('trick', [])]
            trump = [SHORT[e['trump']]] if e.get('trump') in SHORT else []
            truth.append((e['frame'], [set(hand), set(trump), set(table)]))
        c = collections.Counter()
        for i, (f, roles) in enumerate(truth):
            rs = reads.get((g, f), [])
            everywhere = set().union(*roles)
            for role, card, m in rs:
                c['letture'] += 1
                if card not in everywhere:
                    c['identita_sbagliate'] += 1
                    c['identita_sbagliate_sicure'] += m >= AUTO
                elif card not in roles[role]:
                    c['ruolo_sbagliato'] += 1
            prev = truth[i - 1][1] if i else [set(), set(), set()]
            for role in range(3):
                for card in roles[role] - prev[role]:
                    # La prima lettura nuova nel ruolo, entro la finestra.
                    first = None
                    for j in range(i, min(i + window, len(truth))):
                        fj, rj = truth[j]
                        if card not in rj[role]:
                            break
                        new = [(cc, m) for (ro, cc, m) in reads.get((g, fj), []) if ro == role and cc not in prev[role]]
                        if new:
                            hit = [x for x in new if x[0] == card]
                            first = hit[0] if hit else max(new, key=lambda x: x[1])
                            break
                    c['comparse'] += 1
                    by_role[role]['comparse'] += 1
                    if first is None:
                        c['mancate'] += 1
                        by_role[role]['mancate'] += 1
                    elif first[0] == card:
                        c['giuste'] += 1
                        c['giuste_sicure'] += first[1] >= AUTO
                        by_role[role]['giuste'] += 1
                        by_role[role]['giuste_sicure'] += first[1] >= AUTO
                    else:
                        c['sbagliate'] += 1
                        c['sbagliate_sicure'] += first[1] >= AUTO
                        by_role[role]['sbagliate'] += 1
        print(f"{g}: comparse {c['comparse']} giuste {c['giuste']} sicure {c['giuste_sicure']} sbagliate {c['sbagliate']} "
              f"(sicure {c['sbagliate_sicure']}) mancate {c['mancate']} · letture {c['letture']} identità sbagliate sicure "
              f"{c['identita_sbagliate_sicure']} ruolo sbagliato {c['ruolo_sbagliato']}")
        tot.update(c)
    pc = lambda a, b: 100.0 * a / max(b, 1)
    print(f"== PRIMA LETTURA (finestra {window}): comparse {tot['comparse']} · giuste {pc(tot['giuste'], tot['comparse']):.1f}% · "
          f"giuste e sicure {pc(tot['giuste_sicure'], tot['comparse']):.1f}% · sbagliate {tot['sbagliate']} "
          f"(sicure {tot['sbagliate_sicure']}) · mancate {tot['mancate']}")
    print(f"== LETTURE: {tot['letture']} · identità sbagliate {tot['identita_sbagliate']} (sicure "
          f"{tot['identita_sbagliate_sicure']}, {pc(tot['identita_sbagliate_sicure'], tot['letture']):.2f}%) · ruolo sbagliato "
          f"{tot['ruolo_sbagliato']}")
    for role, label in enumerate(['mano', 'briscola', 'tavolo']):
        b = by_role[role]
        print(f"== {label}: comparse {b['comparse']} · giuste {pc(b['giuste'], b['comparse']):.1f}% · sicure "
              f"{pc(b['giuste_sicure'], b['comparse']):.1f}% · sbagliate {b['sbagliate']} · mancate {b['mancate']}")


if __name__ == '__main__':
    main()
