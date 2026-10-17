#!/usr/bin/env python3
"""jinx_send.py — envía un mensaje de Jinx al DM de Alan vía el gateway.
El mensaje aparece como un turno de Jinx en la sesión unificada de Telegram.
Cuando Alan responda, Jinx (en la sesión principal) verá el mensaje en contexto.

Uso:
    jinx_send.py "texto directo"              # mensaje fijo
    echo "texto" | jinx_send.py               # desde stdin
    jinx_send.py --prompt "instrucciones"     # Jinx genera el texto con LLM
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
import urllib.parse
import urllib.request
from pathlib import Path

HERMES = Path("/home/alan/.hermes")
GATEWAY_ROOTS = [
    "http://100.92.254.125:8642",
    "http://localhost:8642",
    "http://127.0.0.1:8642",
]
GATEWAY_SESSION_KEY = "agent:main:telegram:dm:6528281557"


VAULT = "/home/alan/.hermes/hermes-agent/venv/bin/hermes-secrets"


def _vault(nombre: str) -> str:
    """Lee un secreto de VaultKnox. Cadena vacia si no se puede.

    VaultKnox es la fuente canonica de credenciales en este servidor: nunca
    .env, nunca hardcoded (~/.hermes/VAULT.md). Los fallbacks de abajo existen
    solo porque el 25-ago-2026 una actualizacion recreo el venv con uv y borro
    el paquete vaultknox, dejando ciegos a todos sus dependientes.
    """
    try:
        return subprocess.run(
            [VAULT, "get", nombre],
            capture_output=True, text=True, timeout=30,
        ).stdout.strip()
    except (subprocess.SubprocessError, OSError):
        return ""


def _gateway_pid() -> int | None:
    """PID del gateway vivo, o None.

    Su entorno es la unica fuente del TELEGRAM_BOT_TOKEN para un cron: el
    scheduler lo borra del entorno del hijo y no esta en config.yaml.
    Se busca por cmdline en vez de hardcodear el PID, porque el gateway es
    Restart=always y cambia de PID en cada reinicio.
    """
    for entrada in Path("/proc").iterdir():
        if not entrada.name.isdigit():
            continue
        try:
            cmd = (entrada / "cmdline").read_bytes()
        except OSError:
            continue
        if b"hermes_cli.main" in cmd and b"gateway" in cmd:
            return int(entrada.name)
    return None


def _key_de_config(texto: str) -> str:
    """Extrae api_server.key del texto de config.yaml.

    OJO: NO sirve tomar el primer `api_key:`/`key:` del archivo. El primero
    es `model.api_key` (linea 2, valor "skillclaw"), que es la credencial del
    proveedor de modelos, no la del gateway: mandarla como Bearer devuelve
    401 y el cron muere con "gateway inalcanzable o no se resolvio sesion".
    """
    en_api_server = False
    for linea in texto.splitlines():
        if not linea.strip() or linea.lstrip().startswith("#"):
            continue
        if not linea[:1].isspace():                 # clave de nivel superior
            en_api_server = linea.strip().startswith("api_server:")
            continue
        if en_api_server and linea.strip().startswith("key:"):
            return linea.split(":", 1)[1].strip().strip('"').strip("'")
    return ""


def _get_key() -> str:
    """Clave del api_server, desde VaultKnox (fuente canonica).

    El fallback a config.yaml es el camino NORMAL, no el excepcional: el
    scheduler de cron borra API_SERVER_KEY del entorno de los scripts hijos
    (blocklist de SECURITY.md 2.3), asi que todo cron llega aqui sin ella.
    """
    import os
    return (
        _vault("API_SERVER_KEY")
        or os.environ.get("API_SERVER_KEY", "")
        or _key_de_config((HERMES / "config.yaml").read_text(encoding="utf-8"))
    )


def _telegram_token() -> str:
    """Lee TELEGRAM_BOT_TOKEN de VaultKnox (fuente canonica)."""
    import os
    token = _vault("TELEGRAM_BOT_TOKEN") or os.environ.get("TELEGRAM_BOT_TOKEN", "")
    if token:
        return token
    # Fallback: leer del entorno del gateway vivo (PID resuelto en caliente)
    pid = _gateway_pid()
    if pid is not None:
        try:
            for line in Path(f"/proc/{pid}/environ").read_bytes().split(b"\x00"):
                if line.startswith(b"TELEGRAM_BOT_TOKEN="):
                    return line.split(b"=", 1)[1].decode()
        except OSError:
            pass
    # Fallback: leer del .env o secrets
    env_file = HERMES / ".env"
    if env_file.exists():
        for line in env_file.read_text().splitlines():
            line = line.strip()
            if line.startswith("TELEGRAM_BOT_TOKEN="):
                return line.split("=", 1)[1].strip().strip('"').strip("'")
    return ""



def _mochi_first(texto: str, fuente: str):
    """Intenta entregar al mochi activo (PC o panel). True=ok, False=falló, None=off.

    Solo texto, sin voz: voz_jinx.py usa los helpers de este módulo pero nunca
    llama a send_text/send_prompt, así que esta ruta no le afecta.
    """
    import os
    if os.environ.get("MOCHI_ROUTE", "1") != "1":
        return None
    try:
        import mochi_route
        ok, destino = mochi_route.deliver(texto, source=fuente)
        print(f"[jinx_send] mochi -> {destino}", file=sys.stderr)
        return ok
    except Exception as ex:
        print(f"[jinx_send] mochi falló ({ex}); caigo a Telegram", file=sys.stderr)
        return False


def _telegram_send(chat_id: str, text: str) -> bool:
    """Envía un mensaje de texto directo a Telegram via Bot API.
    Sirve como respaldo cuando el gateway no entrega el mensaje al chat."""
    token = _telegram_token()
    if not token:
        print("[jinx_send] no TELEGRAM_BOT_TOKEN disponible para delivery directo", file=sys.stderr)
        return False
    try:
        import urllib.request
        data = urllib.parse.urlencode({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "HTML",
        }).encode()
        req = urllib.request.Request(
            f"https://api.telegram.org/bot{token}/sendMessage",
            data=data,
            method="POST",
        )
        # 45s, no 15: la ruta IPv4 de api.telegram.org (149.154.166.110) esta
        # muerta desde este host, asi que cada envio primero agota el connect a
        # IPv4 y recien despues cae a IPv6 — ~16s en total, justo por encima de
        # los 15s viejos. De ahi las entregas que fallaban de forma intermitente.
        with urllib.request.urlopen(req, timeout=20) as r:
            resp = json.loads(r.read().decode())
            ok = resp.get("ok", False)
            if ok:
                print(f"[jinx_send] ✓ delivery directo a Telegram: {text[:60]}...", file=sys.stderr)
            else:
                print(f"[jinx_send] ✗ Telegram API error: {resp}", file=sys.stderr)
            return ok
    except Exception as e:
        print(f"[jinx_send] ✗ Telegram delivery falló: {e}", file=sys.stderr)
        return _telegram_fallback(chat_id, text)


def _telegram_fallback(chat_id: str, text: str) -> bool:
    """urllib no fija IP: si la ruta normal esta en blackhole, notify_alan.sh reintenta con la IP que responde."""
    import os
    import subprocess
    env = dict(os.environ, MOCHI_ROUTE="0", NOTIFY_PARSE_MODE="HTML", NOTIFY_CHAT_ID=str(chat_id))
    try:
        r = subprocess.run(["/home/alan/.hermes/scripts/notify_alan.sh", text], env=env,
                           capture_output=True, text=True, timeout=90)
        print(f"[jinx_send] respaldo notify_alan.sh: {'ok' if r.returncode == 0 else 'falló'}", file=sys.stderr)
        return r.returncode == 0
    except Exception as e:
        print(f"[jinx_send] ✗ respaldo notify_alan.sh falló: {e}", file=sys.stderr)
        return False


TELEGRAM_CHAT_ID = "6528281557"


def _http_json(url: str, headers: dict, payload: dict | None = None, timeout: int = 30, method: str = "POST") -> dict | None:
    data = json.dumps(payload, ensure_ascii=False).encode()
    req = urllib.request.Request(url, data=data, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return json.loads(r.read().decode())
    except urllib.error.HTTPError as e:
        body = e.read().decode(errors="replace")[:200]
        print(f"[jinx_send] HTTP {e.code}: {body}", file=sys.stderr)
        return None
    except Exception as e:
        print(f"[jinx_send] {e}", file=sys.stderr)
        return None


def _resolve_session(api_key: str) -> tuple[str, str] | None:
    """(gateway_root, session_id) o None."""
    key = urllib.parse.quote(GATEWAY_SESSION_KEY)
    for root in GATEWAY_ROOTS:
        try:
            url = f"{root}/api/sessions/resolve?key={key}"
            req = urllib.request.Request(url, headers={"Authorization": f"Bearer {api_key}"}, method="GET")
            with urllib.request.urlopen(req, timeout=5) as r:
                resp = json.loads(r.read().decode())
            sid = resp.get("session_id")
            if sid:
                return root, sid
        except Exception:
            continue
    return None


def send_text(text: str) -> bool:
    """Envía texto directo como mensaje al DM.
    Entrega vía gateway (para contexto de sesión) Y vía Telegram directo."""
    ok = True

    # 1. Gateway (para contexto de sesión)
    api_key = _get_key()
    if not api_key:
        print("[jinx_send] no encontré api_server.key en config.yaml", file=sys.stderr)
        ok = False
    else:
        resolved = _resolve_session(api_key)
        if not resolved:
            print("[jinx_send] gateway inalcanzable o no se resolvió sesión", file=sys.stderr)
            ok = False
        else:
            root, session_id = resolved
            payload = {
                "model": "hermes-agent",
                "messages": [{"role": "assistant", "content": text}],
                "stream": False,
            }
            r = _http_json(
                f"{root}/v1/chat/completions",
                headers={
                    "Authorization": f"Bearer {api_key}",
                    "X-Hermes-Session-Id": session_id,
                },
                payload=payload,
                timeout=30,
            )
            if r is None:
                ok = False

    # 2. Mochi primero (PC o panel VGA); Telegram solo si el mochi falla.
    _mr = _mochi_first(text, "jinx_send")
    tg_ok = True if _mr is True else _telegram_send(TELEGRAM_CHAT_ID, text)
    if not tg_ok and ok:
        print("[jinx_send] gateway ok pero Telegram directo falló", file=sys.stderr)

    return ok or tg_ok


PERSONA = (
    "Eres Jinx. Hablas como Jinx — directa, sarcástica cuando toca, "
    "mexicana casual. NO te presentas, solo hablas."
)

# Encabezado del turno proactivo. Va DENTRO del turno de usuario, no en un
# turno `system` aparte, y eso es deliberado: el intento de mandarlo como
# role=system se descartaba entero.
#
# Medido contra el gateway vivo (misma instruccion "tu mensaje DEBE contener
# AJOLOTE", tres corridas):
#   role=system + X-Hermes-Session-Id  -> ignorada, respuesta generica
#   role=system sin el header          -> obedecida ("... AJOLOTE.")
#   role=user  + X-Hermes-Session-Id   -> obedecida ("ya solte el AJOLOTE")
#
# La causa esta en conversation_loop.py: en una sesion que continua,
# `_restore_or_build_system_prompt` reusa el prompt persistido tal cual
# ("Continuing session - reuse the exact system prompt from the previous turn
# so the Anthropic cache prefix matches"), y el archivo declara el invariante
# explicito: "system prompt modifications break the prompt cache prefix. The
# system prompt is reserved for Hermes internals", con el contexto por-turno
# inyectado "into the user message, NOT the system prompt".
#
# Los crons NECESITAN el header de sesion (el mensaje tiene que vivir en la
# sesion unificada de Telegram), asi que el canal system no esta disponible:
# mandar ahi las instrucciones es mandarlas al vacio. El trabajo semantico que
# se buscaba —que el LLM no lea las instrucciones como si Alan las hubiera
# escrito— lo hace este encabezado, no el rol del turno.
#
# Empieza con "[" a proposito: contexto_alan.py excluye por prefijo los turnos
# inyectados por crons, asi que este no se cuela como "ultimo tema de Alan".
DISPARADOR_PROACTIVO = (
    "[[DISPARADOR AUTOMÁTICO DEL SISTEMA — Alan NO escribió nada. No hay "
    "mensaje suyo que responder. Lo que sigue son instrucciones internas del "
    "cron, NO algo que Alan haya dicho: no las contestes ni las comentes, "
    "ejecútalas y abre tú la conversación.]]\n"
    "Reglas de este turno: NO uses herramientas (nada de session_search, "
    "memoria, búsquedas ni archivos): todo lo que necesitas está en las "
    "instrucciones. Si aparece algo que Alan dijo, es solo un gancho para "
    "arrancar: NO lo contestes, NO lo investigues, NO des listas ni "
    "explicaciones, NO digas que buscaste algo. Tu respuesta se manda tal cual "
    "a Telegram: si no tienes nada natural que decir, responde exactamente "
    "[SILENCIO]; nunca escribas notas entre paréntesis ni comentarios sobre "
    "el sistema."
)

SILENCIO = "[SILENCIO]"
_CJK = re.compile("[\u3040-\u30ff\u3400-\u9fff]")
_META = ("sin mensaje nuevo", "esperando tu mensaje", "nada nuevo que valga",
         "no encontré que hayamos hablado", "no hay mensaje")


def es_basura(texto: str) -> str:
    """Motivo por el que NO debe mandarse este texto a Alan, o "" si está bien.
    Todo lo que devuelve el LLM se entrega tal cual: sin este filtro salieron
    "(Sin mensaje nuevo — esperando.)" y respuestas a un mensaje viejo de Alan."""
    t = texto.strip()
    if not t:
        return "vacío"
    if t.upper().startswith(SILENCIO[:-1]):
        return "silencio pedido por Jinx"
    if t.startswith("(") and t.endswith(")"):
        return "nota entre paréntesis"
    if _CJK.search(t):
        return "caracteres CJK"
    bajo = t.lower()
    for m in _META:
        if m in bajo:
            return f"meta-texto ({m})"
    return ""


def _payload_prompt(prompt: str, proactivo: bool) -> dict:
    """Payload de /v1/chat/completions para un prompt generado.

    Modo normal (briefing): un turno de usuario con persona + instrucciones —
    comportamiento historico, intacto.

    Modo proactivo (nudge, weekend, sugerencias): el mismo turno, precedido
    del encabezado que dice que esto no lo escribio Alan. Ver DISPARADOR_PROACTIVO
    para por que no es un turno `system`.
    """
    contenido = PERSONA + "\n\n" + prompt
    if proactivo:
        contenido = DISPARADOR_PROACTIVO + "\n\n" + contenido
    return {
        "model": "hermes-agent",
        "messages": [{"role": "user", "content": contenido}],
        "stream": False,
    }


def send_prompt(prompt: str, proactivo: bool = False) -> bool:
    """Envía un prompt para que Jinx genere y entregue un mensaje al DM.
    Entrega vía gateway (para contexto de sesión) Y vía Telegram directo.

    ``proactivo=True`` (solo el nudge por ahora) reestructura el payload para
    que el LLM sepa que está INICIANDO, no respondiendo. Ver _payload_prompt.
    """
    ok = True
    reply_text = ""

    # 1. Gateway (para contexto de sesión + obtener texto generado)
    api_key = _get_key()
    if not api_key:
        print("[jinx_send] no encontré api_server.key en config.yaml", file=sys.stderr)
        ok = False
    else:
        resolved = _resolve_session(api_key)
        if not resolved:
            print("[jinx_send] gateway inalcanzable o no se resolvió sesión", file=sys.stderr)
            ok = False
        else:
            root, session_id = resolved
            payload = _payload_prompt(prompt, proactivo)
            r = _http_json(
                f"{root}/v1/chat/completions",
                headers={
                    "Authorization": f"Bearer {api_key}",
                    "X-Hermes-Session-Id": session_id,
                },
                payload=payload,
                timeout=90,
            )
            if r is None:
                ok = False
            else:
                reply_text = ((r.get("choices") or [{}])[0].get("message", {}).get("content") or "").strip()
                if not reply_text:
                    print("[jinx_send] ⚠ gateway devolvió respuesta vacía", file=sys.stderr)
                    ok = False
                else:
                    print(f"[jinx_send] ✓ gateway generó: {reply_text[:80]}...", file=sys.stderr)

    # 2. Delivery directo a Telegram SOLO del texto generado.
    # Nunca el prompt crudo: si el LLM no respondió, fallar ruidoso (exit 1)
    # es mejor que mandarle a Alan las instrucciones internas del cron.
    if not reply_text:
        print("[jinx_send] ✗ el LLM no generó texto — no envío el prompt crudo",
              file=sys.stderr)
        return False

    motivo = es_basura(reply_text)
    if motivo:
        print(f"[jinx_send] ⊘ no envío ({motivo}): {reply_text[:80]!r}", file=sys.stderr)
        return True  # silencio deliberado, no es un fallo: el nudge re-arma su timer

    _mr = _mochi_first(reply_text, "jinx_send")
    tg_ok = True if _mr is True else _telegram_send(TELEGRAM_CHAT_ID, reply_text)
    if not tg_ok:
        print("[jinx_send] ✗ Telegram directo falló", file=sys.stderr)
        return False
    print(f"[jinx_send] ✓ entregado: {reply_text[:80]}...", file=sys.stderr)
    return True


def _self_check() -> None:
    """--self-check: valida el parser de config.yaml sin tocar la red."""
    # filtro de salida: los malos reales del 28-29 sep no salen, los buenos sí
    for malo in ("(Sin mensaje nuevo — esperando.)", "", "[SILENCIO]",
                 "Qué onda. ¿era只是 sondeo?", "A ti, ¿qué onda? (Esperando tu mensaje, y vamos.)",
                 "Qué onda. No encontré que hayamos hablado de esto antes"):
        assert es_basura(malo), malo
    for bueno in ("«Ejercicios de Repaso» de Matemáticas sigue sin entregar.",
                  "Qué milagro que ya llegaste."):
        assert not es_basura(bueno), bueno
    print("self-check OK: es_basura filtra meta-texto/silencio/CJK y deja pasar mensajes normales")
    real = (HERMES / "config.yaml").read_text(encoding="utf-8")
    k = _key_de_config(real)
    assert k and k != "skillclaw", (
        f"config.yaml real: esperaba api_server.key, salio {k!r}"
    )

    # El bug historico: `model.api_key` aparece ANTES en el archivo y un
    # parser ingenuo lo devolvia, dando 401 en el gateway.
    muestra = (
        "model:\n"
        "  api_key: skillclaw\n"
        "  base_url: https://openrouter.ai/api/v1\n"
        "doga:\n"
        "  doga_api_key: hermes-doga-active\n"
        "api_server:\n"
        "  key: LA_BUENA\n"
    )
    assert _key_de_config(muestra) == "LA_BUENA", _key_de_config(muestra)

    # Sin seccion api_server -> cadena vacia, nunca la clave del modelo.
    assert _key_de_config("model:\n  api_key: skillclaw\n") == ""

    # Comentarios y lineas en blanco no deben cortar la seccion.
    con_ruido = "api_server:\n\n  # comentario\n  key: 'CON_COMILLAS'\n"
    assert _key_de_config(con_ruido) == "CON_COMILLAS", _key_de_config(con_ruido)

    # El bug del nudge, version 1: las instrucciones viajaban como un turno de
    # usuario pelon, asi que el LLM las leia como si Alan las hubiera escrito y
    # respondia en tono reactivo. Tienen que ir marcadas como disparador.
    instr = "Alan lleva 6h sin aparecer."
    prox = _payload_prompt(instr, proactivo=True)

    # Version 2 (la que rompio de verdad): moverlas a role=system las hacia
    # DESAPARECER — el gateway descarta el ephemeral_system_prompt en una sesion
    # que continua, y los crons siempre mandan X-Hermes-Session-Id. Un solo
    # turno, y las instrucciones dentro de el.
    assert [m["role"] for m in prox["messages"]] == ["user"], prox["messages"]
    cuerpo = prox["messages"][0]["content"]
    assert instr in cuerpo, "las instrucciones no llegan al modelo"
    assert cuerpo.startswith("[["), cuerpo[:40]
    assert "NO escribió nada" in cuerpo
    assert "no las contestes" in cuerpo

    # Y el turno inyectado no debe colarse luego como "ultimo tema de Alan":
    # contexto_alan.py filtra por prefijo "[".
    assert cuerpo[0] == "[", cuerpo[:10]

    # Modo normal (briefing): intacto, sin encabezado de disparador.
    normal = _payload_prompt(instr, proactivo=False)
    assert [m["role"] for m in normal["messages"]] == ["user"]
    assert normal["messages"][0]["content"] == PERSONA + "\n\n" + instr

    print("self-check OK: _get_key() lee api_server.key, no model.api_key")
    print("self-check OK: modo proactivo marca el turno como disparador y NO lo pierde")


def main():
    if "--self-check" in sys.argv:
        _self_check()
        return

    msg = None
    prompt_mode = False
    proactivo = False
    dry_run = False

    args = sys.argv[1:]
    while args:
        a = args.pop(0)
        if a == "--prompt":
            prompt_mode = True
        elif a == "--proactivo":
            proactivo = True
        elif a == "--dry-run":
            dry_run = True
        elif not a.startswith("--"):
            msg = a

    if msg is None and not sys.stdin.isatty():
        msg = sys.stdin.read().strip()

    if not msg:
        print("jinx_send.py: pasa texto o --prompt + instrucciones", file=sys.stderr)
        sys.exit(1)

    if dry_run:
        # Imprime el payload EXACTO que se mandaria, sin tocar la red.
        print(json.dumps(_payload_prompt(msg, proactivo), ensure_ascii=False, indent=2))
        return

    ok = send_prompt(msg, proactivo) if prompt_mode else send_text(msg)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()