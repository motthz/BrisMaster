"""Confronta le soglie per ruolo su replay distinti, usando le righe margine."""
import argparse
import re
from pathlib import Path

LINE = re.compile(r"^margine (\d+) (\w+) (\d+) (\d+) (\S+)$")


def read(path):
    text = path.read_text(encoding="utf-8-sig")
    if not re.search(r"test result: ok\. 1 passed; 0 failed;", text):
        raise ValueError(f"Replay incompleto: {path}")
    out = {}
    for line in text.splitlines():
        line = re.sub(r"^test [\w:]+ \.\.\. ", "", line)
        row = LINE.fullmatch(line)
        if not row:
            continue
        stamp, role, expected, found, margin = row.groups()
        group = role.rstrip("0123456789")
        key = (stamp, role)
        if key in out:
            raise ValueError(f"Riga ripetuta: {key}")
        out[key] = (group, expected == found, float(margin))
    if not out:
        raise ValueError(f"Mancano le righe margine in {path}")
    return out


def cost(samples, threshold):
    errors = sum(not correct and margin >= threshold for correct, margin in samples)
    manual = sum(correct and margin < threshold for correct, margin in samples)
    return errors, manual


def regresses(samples, old, new):
    return any(
        ((correct and old <= margin < new)
         or (not correct and new <= margin < old))
        for correct, margin in samples
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("calibrazione", type=Path)
    parser.add_argument("verifica", type=Path)
    args = parser.parse_args()
    calibration, validation = read(args.calibrazione), read(args.verifica)
    if set(calibration) & set(validation):
        parser.error("Calibrazione e verifica devono usare schermate distinte")
    for group in sorted({row[0] for row in calibration.values()}):
        train, test = ([row[1:] for row in source.values() if row[0] == group]
                       for source in (calibration, validation))
        old = 1.4
        options = [n / 20 for n in range(20, 81) if not regresses(train, old, n / 20)]
        chosen = min(options, key=lambda t: (cost(train, t), abs(old - t)))
        if not test or regresses(test, old, chosen):
            chosen = old
        print(f"{group} soglia {chosen:.2f} su {len(train)} letture di calibrazione e "
              f"{len(test)} di verifica, (errori automatici, conferme) {cost(test, old)} -> {cost(test, chosen)}")


if __name__ == "__main__":
    main()
