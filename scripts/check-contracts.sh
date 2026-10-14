#!/usr/bin/env bash
# scripts/check-contracts.sh — los vectores que fijan el comportamiento compartido entre la PC (Rust) y el teléfono
# (Kotlin) deben ser idénticos byte a byte. Falla (exit 1) si alguno difiere o falta.
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"
bad=0
check() {
  local a="$root/$1" b="$root/$2"
  if [ ! -f "$a" ] || [ ! -f "$b" ]; then echo "FALTA: $1 o $2"; bad=1; return; fi
  if cmp -s "$a" "$b"; then echo "ok: $(basename "$1")"; else echo "DIFIERE: $1 <-> $2"; bad=1; fi
}
check windows/scripts/phone/merge-vectors.json    android/app/src/test/resources/merge-vectors.json
check windows/scripts/phone/presence-vectors.json android/app/src/test/resources/presence-vectors.json
exit "$bad"
