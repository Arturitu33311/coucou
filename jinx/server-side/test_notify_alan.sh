#!/bin/bash
# Prueba notify_alan.sh con stubs: ni mochi ni Telegram reales.
set -u
cd "$(dirname "$0")"
T=$(mktemp -d)
mkdir -p "$T/bin"
printf '#!/bin/bash\necho faketoken\n' > "$T/secrets"; chmod +x "$T/secrets"
# mochi stub: MOCHI_STUB=ok|fail
printf '#!/bin/bash\nif [ "${MOCHI_STUB:-ok}" = ok ]; then echo s22; exit 0; else echo "FAIL: nadie mirando (vga)"; exit 1; fi\n' > "$T/mochi"; chmod +x "$T/mochi"
# curl stub: cuenta las llamadas y contesta ok
printf '#!/bin/bash\necho x >> %s/curl.calls\nwhile [ $# -gt 0 ]; do [ "$1" = -o ] && out="$2"; shift; done\necho "{\\"ok\\":true}" > "$out"; echo -n 200\n' "$T" > "$T/bin/curl"; chmod +x "$T/bin/curl"
sed -e "s|/home/alan/.hermes/hermes-agent/venv/bin/hermes-secrets|$T/secrets|" \
    -e "s|/home/alan/.hermes/scripts/mochi_route.py|$T/mochi|" notify_alan.sh > "$T/n.sh"
run() { rm -f "$T/curl.calls"; out=$(env PATH="$T/bin:$PATH" "$@" bash "$T/n.sh" "$MSG" 2>/dev/null); echo "  -> [$out] telegram_calls=$(wc -l < "$T/curl.calls" 2>/dev/null || echo 0)"; }
fail=0
check() { [ "$1" = "$2" ] || { echo "FALLA: $3 (esperaba $2, hubo $1)"; fail=1; }; }

MSG="hola info"
echo "info, mochi ok:";            run MOCHI_STUB=ok;                          calls=$(wc -l < "$T/curl.calls" 2>/dev/null || echo 0); check "$calls" 0 "info solo mochi"
echo "info + ALSO_TELEGRAM:";      run MOCHI_STUB=ok NOTIFY_ALSO_TELEGRAM=1;   calls=$(wc -l < "$T/curl.calls"); check "$calls" 1 "briefing a los dos"; case "$out" in *"telegram + mochi:s22"*) ;; *) echo "FALLA ruta: $out"; fail=1;; esac
echo "info, mochi falla:";         run MOCHI_STUB=fail;                        calls=$(wc -l < "$T/curl.calls"); check "$calls" 1 "fallback a Telegram"
MSG="⚠️ alerta"
echo "warn, mochi ok:";            run MOCHI_STUB=ok;                          calls=$(wc -l < "$T/curl.calls"); check "$calls" 1 "warn a los dos"
echo "MOCHI_ROUTE=0:";             MSG="hola"; run MOCHI_ROUTE=0;              calls=$(wc -l < "$T/curl.calls"); check "$calls" 1 "sin mochi"
rm -rf "$T"
[ $fail = 0 ] && echo "TODO BIEN" || { echo "HUBO FALLOS"; exit 1; }
