"""Confronta i casi individuali di siti_possibili e tavoli_varianti."""
import argparse
from pathlib import Path
import re


ROW = re.compile(
    r"PARTITA (siti_possibili|tavoli_varianti) (.+) seed=(\d+) "
    r"completa=(true|false) conferme=(Some\(\d+\)|None) attribuzioni=(\d+) ritardi=(\d+)"
)


def read_rows(path):
    rows = {}
    for line in path.read_text(encoding="utf-8-sig").splitlines():
        match = ROW.fullmatch(line.strip())
        if not match:
            continue
        bank, name, seed, completed, confirmations, wrong, delayed = match.groups()
        key = bank, name, int(seed)
        if key in rows:
            raise ValueError(f"{path}: caso duplicato {key}")
        rows[key] = {
            "fallita": int(completed == "false"),
            "conferme": None if confirmations == "None" else int(confirmations[5:-1]),
            "attribuzioni": int(wrong),
            "ritardi": int(delayed),
        }
    if not rows:
        raise ValueError(f"{path}: nessun risultato individuale")
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    args = parser.parse_args()
    try:
        baseline = read_rows(args.baseline)
        candidate = read_rows(args.candidate)
    except (OSError, ValueError) as error:
        parser.error(str(error))

    missing = baseline.keys() - candidate.keys()
    extra = candidate.keys() - baseline.keys()
    for case in sorted(missing):
        print("MANCANTE", *case)
    for case in sorted(extra):
        print("AGGIUNTO", *case)

    worse = better = 0
    for case in sorted(baseline.keys() & candidate.keys()):
        before, after = baseline[case], candidate[case]
        for metric, old in before.items():
            new = after[metric]
            if old is None or new is None or old == new:
                continue
            regression = new > old
            worse += regression
            better += not regression
            print("PEGGIORA" if regression else "MIGLIORA", *case, metric, old, "->", new)

    print(f"{len(baseline)} casi di riferimento, {better} misure migliorate, {worse} peggiorate")
    return int(bool(missing or worse))


if __name__ == "__main__":
    raise SystemExit(main())
