"""Compare paired full-game replay logs from window_changes_during_whole_games."""
import argparse
import json
import re
from pathlib import Path

ROW = re.compile(r"^seed=(\d+) scenario=(\w+) punti_veri=\[[^\]]+\] (\{.*\})$")
HIGHER_IS_BETTER = ("correct", "correct_identified", "ready_with_correct_position")
COUNTERS = (*HIGHER_IS_BETTER, "wrong", "wrong_identified", "ready_with_wrong_position")


def read_games(paths):
    games = {}
    for path in paths:
        text = path.read_text()
        if "test result: ok. 1 passed" not in text:
            raise ValueError(f"Replay incompleto in {path}")
        for line in text.splitlines():
            match = ROW.fullmatch(line)
            if not match:
                continue
            key = int(match[1]), match[2]
            report = json.loads(match[3])
            if key in games or sum(report[k] for k in ("correct", "wrong", "missing")) != 40:
                raise ValueError(f"Partita duplicata o incompleta: {key}")
            games[key] = report
    if not games:
        raise ValueError("Nessuna partita completa")
    return games


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", nargs="+", required=True, type=Path)
    parser.add_argument("--after", nargs="+", required=True, type=Path)
    args = parser.parse_args()
    before, after = read_games(args.before), read_games(args.after)
    if before.keys() != after.keys():
        raise ValueError("Le due serie devono contenere gli stessi scenari e semi")
    for name in COUNTERS:
        print(name, sum(r[name] for r in before.values()), sum(r[name] for r in after.values()))
    regressions = []
    for key, old in sorted(before.items()):
        new = after[key]
        for name in COUNTERS:
            delta = new[name] - old[name]
            if delta and (delta < 0 if name in HIGHER_IS_BETTER else delta > 0):
                regressions.append((key, name, old[name], new[name]))
    for row in regressions:
        print("REGRESSION", *row)
    print(f"{len(after)} partite, {40 * len(after)} carte, {len(regressions)} regressioni")
    return bool(regressions)


if __name__ == "__main__":
    raise SystemExit(main())
