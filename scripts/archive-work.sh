#!/usr/bin/env bash
# Keep a complete offline Git history plus a plain source snapshot. Never publishes anything.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -n "$(git status --porcelain)" ]]; then
  echo 'Commit or preserve current changes before exporting; the backup must describe a complete checkpoint.' >&2
  exit 1
fi
out="${1:?Usage: bash scripts/archive-work.sh /absolute/path/to/new-backup-directory}"
case "$out" in /*) ;; *) echo 'Use an absolute backup path.' >&2; exit 1;; esac
if [[ -e "$out" ]]; then echo 'Choose a new directory; existing backups are never overwritten.' >&2; exit 1; fi
mkdir -p "$out"
git bundle create "$out/BrisMaster.bundle" --all
git bundle verify "$out/BrisMaster.bundle"
git archive --format=tar.gz --output="$out/BrisMaster-source.tar.gz" HEAD
branch="$(git branch --show-current)"
commit="$(git rev-parse HEAD)"
cat > "$out/RESTORE.txt" <<RESTORE
BrisMaster offline checkpoint
Commit: $commit
Branch: $branch

Full repository (no GitHub or Codex required):
  git clone -b $branch BrisMaster.bundle BrisMaster
  cd BrisMaster
  cargo test --workspace --locked --release
  cargo run --locked --release -p brismaster

Source only:
  mkdir source
  tar -xzf BrisMaster-source.tar.gz -C source

See docs/SVILUPPO-E-RIPRISTINO.md and docs/STATO-LAVORO.md for requirements,
verified behavior, limitations and pending Windows validation.
Dependencies can require Internet access on a clean machine. Trained models and
reference deck images are included; game records and credentials are not.
RESTORE
(cd "$out" && sha256sum BrisMaster.bundle BrisMaster-source.tar.gz RESTORE.txt > SHA256SUMS)
printf 'Verified backup: %s\n' "$out"
