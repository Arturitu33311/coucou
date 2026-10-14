#!/usr/bin/env bash
# scripts/jinx-assist/authorize-phone-pendientes.sh <nombre> '<línea pública ssh-rsa …>'
#
# Autoriza la llave de un teléfono en el SERVER solo para el gateway de pendientes de Jinx
# (coucou_pendientes.py: list / add / resolve). Es una llave con `restrict` y comando forzado: no da shell.
# Lo ejecuta una persona (toca authorized_keys del server); hace copia de seguridad y es idempotente.
set -euo pipefail
name="${1:?nombre del teléfono (s22, s21)}"
key="${2:?línea pública: ssh-rsa AAAA…}"
case "$key" in ssh-rsa\ *|ssh-ed25519\ *) ;; *) echo "eso no parece una llave pública" >&2; exit 1;; esac
case "$name" in *[!A-Za-z0-9_-]*|"") echo "nombre no válido" >&2; exit 1;; esac
body="$(printf '%s' "$key" | awk '{print $1" "$2}')"
host="${JINX_SERVER:-server}"
ssh -o BatchMode=yes "$host" bash -s -- "$name" "$body" <<'REMOTE'
set -euo pipefail
name="$1"; body="$2"
f="$HOME/.ssh/authorized_keys"
cmd="/home/alan/.hermes/hermes-agent/venv/bin/python /home/alan/.hermes/scripts/coucou_pendientes.py"
if grep -qF "${body#* }" "$f"; then echo "ya estaba autorizada"; exit 0; fi
cp -p "$f" "$f.bak-$(date +%Y%m%d-%H%M%S)-coucou-pendientes"
printf 'restrict,command="%s" %s coucou-%s-pendientes\n' "$cmd" "$body" "$name" >> "$f"
echo "autorizada ($name) solo para coucou_pendientes.py"
REMOTE
