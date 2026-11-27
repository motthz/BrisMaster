"""Paired neural-network audit using the existing annotated-screen replay."""

import argparse
import csv
import os
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from types import SimpleNamespace

import sweep

LEVELS = {
    'saturazione': [0, .25, .5, .75, 1],
    'tinta': [-60, -45, -25, 25, 45, 60],
    'luminosita': [.2, .55, 1, 1.45, 2],
    'oscuramento_rosso': [.15, .45, .7, .85, 1],
    'oscuramento_blu': [.15, .45, .7, .85, 1],
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=lambda path: str(Path(path).resolve()))
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    sweep.args = SimpleNamespace(binary=args.binary)
    cases = [(key, level, scene) for key, levels in LEVELS.items() for level in levels
             for scene in sweep.SCENES]

    args.output.mkdir(parents=True, exist_ok=True)
    for mode in ('on', 'off'):
        if mode == 'on':
            os.environ.pop('BRIS_TEST_SENZA_RETE', None)
        else:
            os.environ['BRIS_TEST_SENZA_RETE'] = '1'
        with (args.output / f'nn-{mode}.csv').open('w', newline='') as output:
            writer = csv.writer(output)
            writer.writerow(['factor', 'level', 'scene', 'role', 'truth', 'prediction', 'automatic',
                             'extra_slots_in_frame', 'frame_us'])
            with ThreadPoolExecutor(max_workers=2) as pool:
                for rows in pool.map(sweep.run, cases):
                    writer.writerows(rows)


if __name__ == '__main__':
    main()
