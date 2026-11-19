"""Confronta gli output --nocapture dei banchi di riconoscimento."""
import argparse
import re
from pathlib import Path


def read_cases(path, suite, baseline=False):
    cases = {}
    strict = None
    blocked = None
    delayed = {}
    manual = {}
    output = path.read_text(encoding="utf-8-sig")
    for line in output.splitlines():
        # With one test thread libtest can print its prefix on the first log line.
        line = re.sub(r"^test [\w:]+ \.\.\. ", "", line)
        if suite == "fotogrammi":
            card = re.fullmatch(r"lettura (\d+) (\w+) (\d+) (-?\d+) ([01])", line)
            extra = re.fullmatch(r"extra (\d+) (\d+)", line)
            if card:
                time, role, expected, picked, auto = card.groups()
                wrong = expected != picked
                add = (f"{time}/{role}/{expected}", (int(wrong), int(wrong and auto == "1"),
                                                       int(not wrong and auto == "0"), 0))
            elif extra:
                add = (f"{extra[1]}/extra", (0, 0, 0, int(extra[2])))
            else:
                continue
        elif suite == "estreme":
            delay = re.match(r"seed=(\d+); checkpoint in ritardo=(\d+);", line)
            if delay:
                delayed[delay[1]] = int(delay[2])
                continue
            intervention = re.match(r"seed=(\d+); intervento manuale:", line)
            if intervention:
                seed = intervention[1]
                manual[seed] = manual.get(seed, 0) + 1
                continue
            match = re.fullmatch(r"(.+); seed=(\d+); Ok\(\((\d+), (\d+)\)\); [\d.]+s", line)
            if match:
                name, seed, tricks, confirmations = match.groups()
                add = (f"{name}/{seed}", (
                    20 - int(tricks), int(confirmations), manual.pop(seed, 0), delayed.pop(seed, 0),
                ))
            else:
                err = re.fullmatch(r"(.+); seed=(\d+); Err\((.+)\); [\d.]+s", line)
                if not err or not baseline:
                    continue
                add = (f"{err[1]}/{err[2]}", None)
        elif suite == "carte":
            match = re.fullmatch(r"(.+) tavolo (\d+): (.+)", line)
            if not match:
                continue
            deck, table, readings = match.groups()
            fields = readings.split(" · ")
            roles = {"mano0", "mano1", "mano2", "avversario", "tua", "briscola"}
            hand_error = 0
            count = re.fullmatch(r"mano di (\d+) carte invece di (\d+)", fields[-1])
            if count:
                hand_error = abs(int(count[1]) - int(count[2]))
                fields.pop()
            for field in fields:
                role, _, result = field.partition(": ")
                if role not in roles or not result:
                    raise ValueError(f"{path}: lettura inattesa {field}")
                roles.remove(role)
                missing = result == "non vista"
                wrong = missing or "(sbagliata;" in result
                key = f"{deck}/{table}/{role}"
                if key in cases:
                    raise ValueError(f"{path}: caso duplicato {key}")
                cases[key] = (int(wrong), int(wrong or "(da confermare," in result),
                              int(missing), hand_error if role == "mano0" else 0)
            if roles:
                raise ValueError(f"{path}: ruoli mancanti {roles}")
            continue
        elif suite == "mazzi":
            match = re.fullmatch(r"== (.+): giuste (\d+)/(\d+), senza conferma (\d+)/(\d+)", line)
            if not match:
                continue
            name, correct, total, automatic, automatic_total = match.groups()
            if total != automatic_total or int(total) == 0:
                raise ValueError(f"{path}: denominatori incoerenti per {name}")
            add = (name, (int(total) - int(correct), int(total) - int(automatic), int(total)))
        else:
            if line.startswith("  severo: "):
                strict = line.removeprefix("  severo: ")
            latency = re.match(r"  consiglio sospeso: (\d+)/(\d+) letture del mio turno, al più ([\d.]+) s", line)
            if latency:
                blocked = (int(latency[1]), float(latency[3]))
            match = re.fullmatch(
                r"(.+): (\d+) prese su 20, passo (.*?), (\d+) conferme, (\d+) scritte a mano(.*)", line
            )
            if not match:
                continue
            if strict is None or blocked is None:
                raise ValueError(f"{path}: metriche incomplete per {match[1]}")
            name, tricks, step, confirms, typed, details = match.groups()
            gaps = re.search(r"(\d+) ripartenze con prese non viste", strict)
            logged = re.search(r"(\d+) prese registrate", strict)
            add = (name, (
                20 - int(tricks), int(step != "Over"), int(strict != "PERFETTA"),
                int(confirms), int(typed), int(", errori:" in details),
                int("briscola sbagliata" in strict), int("presa " in strict),
                int(gaps[1]) if gaps else 0, 20 - int(logged[1]) if logged else 0,
                *blocked,
            ))
            strict = blocked = None
        name, metrics = add
        if name in cases:
            raise ValueError(f"{path}: caso duplicato {name}")
        cases[name] = metrics

    historical_failure = (baseline and suite == "estreme" and None in cases.values()
                          and re.search(r"test result: FAILED\. 0 passed; 1 failed;", output))
    if not historical_failure and not re.search(r"test result: ok\. 1 passed; 0 failed;", output):
        raise ValueError(f"{path}: il banco deve risultare eseguito con 1 test passato")
    return cases


METRICS = {
    "fotogrammi": ("identità errate o mancanti", "errori automatici", "carte corrette da confermare", "carte in più"),
    "estreme": ("prese mancanti", "interventi totali", "correzioni manuali", "controlli in ritardo"),
    "carte": ("lettura errata", "senza riconoscimento automatico corretto", "posizione mancante", "scarto carte in mano"),
    "mazzi": ("letture non corrette", "letture senza riconoscimento automatico corretto", "campioni"),
    "replay": (
        "prese mancanti", "partita incompleta", "partita imperfetta", "conferme",
        "carte manuali", "errori riportati", "briscola errata", "prese divergenti riportate",
        "ripartenze con lacune", "prese mancanti nello storico", "letture bloccate",
        "secondi consecutivi di blocco",
    ),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("suite", choices=METRICS)
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidata", type=Path)
    parser.add_argument("--casi", type=int, required=True)
    args = parser.parse_args()
    try:
        before = read_cases(args.baseline, args.suite, baseline=True)
        after = read_cases(args.candidata, args.suite)
        if args.casi <= 0 or len(before) != args.casi or before.keys() != after.keys():
            raise ValueError(
                f"Attesi {args.casi} casi identici, trovati {len(before)} e {len(after)}. "
                f"Mancanti: {sorted(before.keys() - after.keys())}. "
                f"Nuovi: {sorted(after.keys() - before.keys())}."
            )
    except (OSError, ValueError) as error:
        parser.exit(2, f"{error}\n")

    failed = False
    for name, old in sorted(before.items()):
        new = after[name]
        if old is None:
            print(f"{name}: RISOLTO")
            continue
        changes = []
        regressed = False
        for metric, left, right in zip(METRICS[args.suite], old, new, strict=True):
            if right != left:
                changes.append(f"{metric} {left} → {right}")
                regressed |= right > left or metric == "campioni"
        failed |= regressed
        status = "PEGGIORATO" if regressed else "MIGLIORATO" if changes else "PARI"
        print(f"{name}: {status}" + (f" ({', '.join(changes)})" if changes else ""))
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
