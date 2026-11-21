"""Identify visible plays absent from the recent journal in BRIS_WINDOW_TRACE logs."""
import argparse
import csv
import json
import re
import sys
from pathlib import Path

FRAME = re.compile(r"^ms=(\d+) presa=(\d+) mano=\[[^]]*\] tavolo=\[([^]]*)\] lettura=.* registro=(.*)$")
ENTRY = re.compile(r'\((\d+), "(io|avversario)", (\d+), (?:true|false)\)')
SUMMARY = re.compile(r"^seed=(\d+) scenario=(\w+) punti_veri=\[[^]]*\] (\{.*\})$")


def inspect(path):
    visible = {}
    recorded = {}
    games = []
    passed = False
    last_ms = 0
    for line in path.read_text().splitlines():
        frame = FRAME.fullmatch(line)
        if frame:
            ms, trick = map(int, frame.group(1, 2))
            if ms <= last_ms:
                raise ValueError(f"{path}: tempi fuori ordine")
            last_ms = ms
            for player, slot in zip(("io", "avversario"), frame[3].split(", "), strict=True):
                if slot != "None":
                    card = int(re.fullmatch(r"Some\((\d+)\)", slot)[1])
                    visible.setdefault((trick, player, card), ms)
            for number, player, card in ENTRY.findall(frame[4]):
                recorded.setdefault((int(number), player, int(card)), ms)
        summary = SUMMARY.fullmatch(line)
        if summary:
            if not visible or json.loads(summary[3])["my_turns"] != 20:
                raise ValueError(f"{path}: traccia assente o partita incompleta")
            games.append((int(summary[1]), summary[2], visible, recorded))
            visible, recorded, last_ms = {}, {}, 0
        passed |= line.startswith("test result: ok. 1 passed;")
    if visible or not games or not passed:
        raise ValueError(f"{path}: replay incompleto")
    return games


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logs", nargs="+", type=Path)
    args = parser.parse_args()
    games = {}
    for path in args.logs:
        for seed, scenario, visible, recorded in inspect(path):
            if (seed, scenario) in games:
                parser.error(f"partita duplicata: {seed}, {scenario}")
            games[seed, scenario] = visible, recorded
    out = csv.writer(sys.stdout)
    out.writerow(["seed", "scenario", "presa", "giocatore", "carta", "visibile_ms", "primo_registro_ms"])
    for (seed, scenario), (visible, recorded) in sorted(games.items()):
        for key, ms in visible.items():
            out.writerow([seed, scenario, *key, ms, recorded.get(key, "")])


if __name__ == "__main__":
    main()
