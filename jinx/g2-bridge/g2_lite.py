"""g2_lite — « Jinx rápida » para las gafas: un modelo pequeño y rápido con su personalidad, sin la memoria ni el
prompt enorme de Hermes, y con las manos justas: Home Assistant (lo que una voz puede pedir sin riesgo) y pasarle
a la Jinx de verdad lo que necesita memoria o herramientas.

Pura y con todo inyectable (modelo, Home Assistant, reloj): los tests no tocan la red. Sin dependencias.

Lo que NO puede hacer desde aquí, a propósito: cerraduras, alarmas, garajes y puertas (`lock`, `alarm_control_panel`,
`cover`), automatizaciones y scripts, reiniciar nada, llamar a servicios fuera de la lista. Una voz mal entendida
(o un texto malicioso que el modelo haya leído) no debe poder abrirte la puerta.
"""
import json
import os
import re
import time
import unicodedata
import urllib.error
import urllib.request

HOME = os.path.expanduser("~")
PERSONA_PATH = os.path.join(HOME, ".config", "jinx-g2", "persona.md")
HA_TOKEN_PATH = os.path.join(HOME, ".config", "jinx-g2", "ha_token")

PERSONA = """Eres Jinx, la asistente de Alan, ahora en sus gafas. Habla en español, natural, con tu carácter (directa, \
cálida, un poco guasona), nunca como un manual.
Lo que dices aparece en una pantalla pequeña de las gafas: UNA o DOS frases cortas, sin listas, sin markdown, sin \
emojis. Si hay que elegir, lo más útil primero.
Tienes herramientas para la casa (Home Assistant): úsalas en vez de suponer. Si no encuentras el aparato, dilo y \
pregunta cuál. Si algo no se puede hacer desde aquí (cerraduras, alarma, garaje), di que eso lo haga él a mano.
No tienes memoria larga ni internet. Si te piden algo que necesita tu memoria, tus notas, buscar, correo, \
programar o tocar el PC, usa la herramienta pasar_a_jinx y dile que se lo pasas; no lo inventes.
No inventes datos: si no lo sabes, dilo."""

MAX_ROUNDS = 4
HISTORY_TURNS = 8        # mensajes (4 idas y vueltas)
HISTORY_IDLE = 10 * 60   # tras diez minutos callados se empieza de cero
REPLY_MAX = 400
TOOL_RESULT_MAX = 1500
FIND_MAX = 8

# Servicio permitido por dominio, y los datos que admite cada uno (con sus límites).
ALLOWED = {
    "light": ("turn_on", "turn_off", "toggle"),
    "switch": ("turn_on", "turn_off", "toggle"),
    "input_boolean": ("turn_on", "turn_off", "toggle"),
    "fan": ("turn_on", "turn_off", "toggle", "set_percentage"),
    "scene": ("turn_on",),
    "media_player": ("media_play", "media_pause", "media_play_pause", "media_next_track", "media_previous_track",
                     "volume_set", "volume_up", "volume_down", "volume_mute", "turn_on", "turn_off"),
    "climate": ("set_temperature", "turn_on", "turn_off"),
}
DATA_RULES = {
    "brightness_pct": (0, 100),
    "color_temp_kelvin": (1500, 6500),
    "percentage": (0, 100),
    "volume_level": (0.0, 1.0),
    "temperature": (16, 30),
}
ENTITY_RE = re.compile(r"^[a-z_]+\.[a-z0-9_]+$")


class LiteError(Exception):
    pass


class HaError(Exception):
    pass


# ---------- política: lo único que se puede pedir a la casa ----------

def check_call(domain, service, entity_id, data):
    """(True, datos limpios) o (False, motivo). Una sola entidad, de ese dominio, con servicio y datos permitidos."""
    if not isinstance(domain, str) or domain not in ALLOWED:
        return False, "el dominio %s no se puede controlar desde las gafas" % (str(domain)[:30],)
    if service not in ALLOWED[domain]:
        return False, "el servicio %s.%s no está permitido" % (domain, str(service)[:30])
    if not isinstance(entity_id, str) or not ENTITY_RE.match(entity_id) or entity_id.split(".")[0] != domain:
        return False, "entity_id debe ser una sola entidad de %s" % domain
    clean = {}
    for k, v in (data if isinstance(data, dict) else {}).items():
        if k == "rgb_color":
            if (isinstance(v, list) and len(v) == 3 and all(isinstance(x, int) and not isinstance(x, bool)
                                                           and 0 <= x <= 255 for x in v)):
                clean[k] = v
                continue
            return False, "rgb_color son tres números 0-255"
        if k == "volume_mute":
            if isinstance(v, bool):
                clean[k] = v
                continue
            return False, "is_volume_muted es verdadero o falso"
        if k not in DATA_RULES:
            return False, "dato no permitido: %s" % str(k)[:30]
        lo, hi = DATA_RULES[k]
        if isinstance(v, bool) or not isinstance(v, (int, float)) or not lo <= v <= hi:
            return False, "%s debe estar entre %s y %s" % (k, lo, hi)
        clean[k] = v
    return True, clean


def _fold(s):
    s = unicodedata.normalize("NFKD", str(s).lower())
    return "".join(c for c in s if not unicodedata.combining(c))


class HomeAssistant:
    """La API REST de Home Assistant, a través de una política fija. `opener` y `now` se pueden cambiar."""

    def __init__(self, url, token_fn, opener=urllib.request.urlopen, now=time.time):
        self.url = url.rstrip("/")
        self.token_fn = token_fn
        self.opener = opener
        self.now = now
        self._cache = (0.0, [])

    def _req(self, method, path, body=None):
        token = self.token_fn()
        if not token:
            raise HaError("no hay llave de Home Assistant en el servidor")
        req = urllib.request.Request(self.url + path, method=method,
                                     data=json.dumps(body).encode() if body is not None else None,
                                     headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
        try:
            with self.opener(req, timeout=8) as r:
                return json.loads(r.read() or b"null")
        except urllib.error.HTTPError as e:
            raise HaError("Home Assistant contestó %d" % e.code)
        except Exception as e:  # noqa: BLE001
            raise HaError("no llego a Home Assistant (%s)" % type(e).__name__)

    def states(self):
        at, items = self._cache
        if self.now() - at < 30 and items:
            return items
        items = self._req("GET", "/api/states") or []
        self._cache = (self.now(), items)
        return items

    def find(self, query, domain=None):
        words = [w for w in _fold(query).split() if w]
        out = []
        for s in self.states():
            eid = s.get("entity_id", "")
            dom = eid.split(".")[0]
            if domain and dom != domain:
                continue
            if dom not in ALLOWED and dom not in ("sensor", "binary_sensor", "weather", "lock", "cover"):
                continue
            name = (s.get("attributes") or {}).get("friendly_name") or eid
            hay = _fold(name + " " + eid)
            if all(w in hay for w in words):
                out.append({"entity_id": eid, "name": name, "state": s.get("state")})
            if len(out) >= FIND_MAX:
                break
        return out

    def state(self, entity_id):
        if not isinstance(entity_id, str) or not ENTITY_RE.match(entity_id):
            raise HaError("entity_id inválido")
        s = self._req("GET", "/api/states/" + entity_id)
        attrs = s.get("attributes") or {}
        keep = {k: attrs[k] for k in ("friendly_name", "brightness", "temperature", "current_temperature",
                                      "unit_of_measurement", "volume_level", "media_title") if k in attrs}
        return {"entity_id": entity_id, "state": s.get("state"), **keep}

    def call(self, domain, service, entity_id, data=None):
        ok, clean = check_call(domain, service, entity_id, data)
        if not ok:
            raise HaError(clean)
        payload = dict(clean, entity_id=entity_id)
        if "volume_mute" in payload:
            payload["is_volume_muted"] = payload.pop("volume_mute")
        self._req("POST", "/api/services/%s/%s" % (domain, service), payload)
        self._cache = (0.0, [])
        return "hecho"


# ---------- las herramientas del modelo ----------

TOOLS = [
    {"type": "function", "function": {
        "name": "ha_buscar", "description": "Busca aparatos de la casa por nombre (luz salón, ventilador, temperatura...).",
        "parameters": {"type": "object", "properties": {
            "consulta": {"type": "string"}, "dominio": {"type": "string", "description": "light, switch, climate, sensor..."}},
            "required": ["consulta"]}}},
    {"type": "function", "function": {
        "name": "ha_estado", "description": "El estado actual de un aparato por su entity_id.",
        "parameters": {"type": "object", "properties": {"entity_id": {"type": "string"}}, "required": ["entity_id"]}}},
    {"type": "function", "function": {
        "name": "ha_accion", "description": "Hace algo con UN aparato: dominio (light, switch, fan, media_player, scene, "
        "input_boolean, climate), servicio (turn_on, turn_off, toggle...), entity_id y, si hace falta, datos "
        "(brightness_pct, color_temp_kelvin, rgb_color, percentage, volume_level, temperature).",
        "parameters": {"type": "object", "properties": {
            "dominio": {"type": "string"}, "servicio": {"type": "string"}, "entity_id": {"type": "string"},
            "datos": {"type": "object"}}, "required": ["dominio", "servicio", "entity_id"]}}},
    {"type": "function", "function": {
        "name": "pasar_a_jinx", "description": "Le pasa la petición a la Jinx completa (con memoria y todas sus "
        "herramientas); ella responde después en la isla o en Telegram. Úsalo para lo que no puedas hacer tú.",
        "parameters": {"type": "object", "properties": {"peticion": {"type": "string"}}, "required": ["peticion"]}}},
]


def clean_reply(text):
    t = re.sub(r"[*_`#>]+", "", text or "")
    t = " ".join(t.split())
    return t if len(t) <= REPLY_MAX else t[: REPLY_MAX - 1] + "…"


class OpenAICompatLLM:
    """Un modelo por una API compatible con OpenAI (OpenRouter por defecto). Lanza LiteError."""

    def __init__(self, url, model, key_fn, opener=urllib.request.urlopen):
        self.url, self.model, self.key_fn, self.opener = url, model, key_fn, opener

    def chat(self, messages, tools):
        key = self.key_fn()
        if not key:
            raise LiteError("sin llave del modelo rápido")
        body = {"model": self.model, "messages": messages, "tools": tools, "temperature": 0.7, "max_tokens": 300}
        req = urllib.request.Request(self.url, data=json.dumps(body).encode(), headers={
            "Content-Type": "application/json", "Authorization": "Bearer " + key})
        try:
            with self.opener(req, timeout=25) as r:
                return json.loads(r.read().decode("utf-8", "replace"))["choices"][0]["message"]
        except Exception as e:  # noqa: BLE001
            raise LiteError("el modelo rápido falló (%s)" % type(e).__name__) from None


class Lite:
    def __init__(self, llm, ha, handoff, persona=PERSONA, now=time.time):
        self.llm, self.ha, self.handoff, self.persona, self.now = llm, ha, handoff, persona, now
        self.history = []
        self.last = 0.0

    def _tool(self, name, args):
        try:
            if name == "ha_buscar":
                return json.dumps(self.ha.find(str(args.get("consulta", "")), args.get("dominio")), ensure_ascii=False)
            if name == "ha_estado":
                return json.dumps(self.ha.state(args.get("entity_id")), ensure_ascii=False)
            if name == "ha_accion":
                return self.ha.call(args.get("dominio"), args.get("servicio"), args.get("entity_id"), args.get("datos"))
            if name == "pasar_a_jinx":
                return self.handoff(str(args.get("peticion", ""))[:1000])
            return "herramienta desconocida"
        except (HaError, LiteError) as e:
            return "error: %s" % e
        except Exception as e:  # noqa: BLE001 - una herramienta rota no tumba la respuesta
            return "error: %s" % type(e).__name__

    def reply(self, text):
        if self.now() - self.last > HISTORY_IDLE:
            self.history = []
        self.last = self.now()
        msgs = [{"role": "system", "content": self.persona}] + self.history + [{"role": "user", "content": text}]
        out = ""
        for _ in range(MAX_ROUNDS):
            m = self.llm.chat(msgs, TOOLS)
            calls = m.get("tool_calls") or []
            if not calls:
                out = clean_reply(m.get("content"))
                break
            msgs.append({"role": "assistant", "content": m.get("content") or "", "tool_calls": calls})
            for c in calls:
                fn = c.get("function") or {}
                try:
                    args = json.loads(fn.get("arguments") or "{}")
                except ValueError:
                    args = {}
                result = self._tool(fn.get("name"), args if isinstance(args, dict) else {})
                msgs.append({"role": "tool", "tool_call_id": c.get("id", ""), "content": result[:TOOL_RESULT_MAX]})
        if not out:
            raise LiteError("el modelo no llegó a una respuesta")
        self.history = (self.history + [{"role": "user", "content": text}, {"role": "assistant", "content": out}]
                        )[-HISTORY_TURNS:]
        return out
