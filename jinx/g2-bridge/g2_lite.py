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

# Persona MÍNIMA para el modelo local (sin OpenRouter): en CPU cada token de prompt cuesta tiempo, así que solo lo justo para que se note
# quién es Jinx y que use las herramientas. El resto de la persona queda para el modelo principal.
PERSONA_LOCAL = """Eres Jinx, la asistente de Alan: directa, cálida, un poco guasona. Español, UNA frase corta, sin markdown ni emojis. \
Para la casa usa las herramientas, no supongas. Si necesita memoria, correo, notas o agenda, usa pasar_a_jinx. No inventes."""

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


_DIAS = ("lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo")
_MESES = ("enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre")


def ahora_texto(ts):
    t = time.localtime(ts)
    return "Ahora: %s %d de %s de %d, %02d:%02d." % (_DIAS[t.tm_wday], t.tm_mday, _MESES[t.tm_mon - 1], t.tm_year, t.tm_hour, t.tm_min)


class LiteError(Exception):
    pass


class HaError(Exception):
    pass


class HaTimeout(HaError):
    """Home Assistant no contestó a tiempo: la orden pudo salir igualmente (una tele tarda en responder al encendido)."""


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


# Cómo se dice lo mismo en casa. La búsqueda ve "nombre + entity_id", y un televisor suele llamarse "Tv ..." o por su
# modelo; sin esto, pedir "la tele" fallaba según la palabra que el modelo eligiera buscar.
_TELE = ("tv", "tele", "televisor", "television", "pantalla")
_HABITACION = ("habitacion", "cuarto", "recamara", "dormitorio", "alcoba")
STOPWORDS = frozenset(("de", "del", "la", "el", "los", "las", "un", "una", "en", "mi", "mis", "tu", "al", "a", "y", "por", "para", "que", "esta", "este", "ese", "esa"))
# Primero lo que se puede controlar; los sensores y el resto, después (así "tele" no devuelve antes un sensor de canal que el aparato).
DOMAIN_RANK = {"media_player": 0, "light": 1, "switch": 2, "fan": 3, "climate": 4, "scene": 5, "input_boolean": 6, "remote": 7}
SINONIMOS = {
    "cuarto": _HABITACION, "recamara": _HABITACION, "dormitorio": _HABITACION, "alcoba": _HABITACION, "habitacion": _HABITACION,
    "tele": _TELE, "televisor": _TELE, "television": _TELE, "tv": _TELE, "pantalla": _TELE,
    "luz": ("luz", "light", "lampara", "foco"), "luces": ("luz", "light", "lampara", "foco"),
    "lampara": ("lampara", "luz", "light"), "foco": ("foco", "luz", "light"),
    "ventilador": ("ventilador", "fan", "abanico"), "abanico": ("abanico", "ventilador", "fan"),
    "bocina": ("bocina", "altavoz", "speaker"), "altavoz": ("altavoz", "bocina", "speaker"),
    "enchufe": ("enchufe", "switch", "plug"), "clima": ("clima", "aire", "climate", "ac"),
    "aire": ("aire", "clima", "climate", "ac"),
}


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

    def _req(self, method, path, body=None, timeout=8):
        token = self.token_fn()
        if not token:
            raise HaError("no hay llave de Home Assistant en el servidor")
        req = urllib.request.Request(self.url + path, method=method,
                                     data=json.dumps(body).encode() if body is not None else None,
                                     headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
        try:
            with self.opener(req, timeout=timeout) as r:
                return json.loads(r.read() or b"null")
        except urllib.error.HTTPError as e:
            raise HaError("Home Assistant contestó %d" % e.code)
        except Exception as e:  # noqa: BLE001
            if isinstance(e, TimeoutError) or "timed out" in str(e).lower():
                raise HaTimeout("Home Assistant tardó en contestar")
            raise HaError("no llego a Home Assistant (%s)" % type(e).__name__)

    def states(self):
        at, items = self._cache
        if self.now() - at < 30 and items:
            return items
        items = self._req("GET", "/api/states") or []
        self._cache = (self.now(), items)
        return items

    def find(self, query, domain=None):
        words = [w for w in _fold(query).split() if w and w not in STOPWORDS] or [w for w in _fold(query).split() if w]
        # Cada palabra puede decirse de otra forma: "tele" encuentra "Tv habitacion", que ninguna palabra suya contiene.
        alts = [SINONIMOS.get(w, (w,)) for w in words]
        return self._find(alts, domain)

    def _find(self, alts, domain):
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
            tokens = set(hay.replace("_", " ").replace(".", " ").split())
            # Una palabra corta ("tv") solo cuenta entera: "INTV 1000" es una caja, no una tele.
            if all(any((a in tokens) if len(a) <= 3 else (a in hay) for a in options) for options in alts):
                out.append({"entity_id": eid, "name": name, "state": s.get("state")})
        out.sort(key=lambda o: (DOMAIN_RANK.get(o["entity_id"].split(".")[0], 9), len(o["name"])))
        return out[:FIND_MAX]

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
        self._req("POST", "/api/services/%s/%s" % (domain, service), payload, timeout=SERVICE_TIMEOUT)
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


_TOOL_WORDS = re.compile(r"^\s*(pasar[_ ]?a[_ ]?jinx|ha[_ ]?(buscar|estado|accion))\b", re.I)
NO_CONFIRMADA = "No pude confirmar que se hiciera: el aparato no respondió. Inténtalo de nuevo."
_EXITO = re.compile(r"\b(listo|hecho|hecha|ya (est[aá]|qued[oó]|lo|la|puse|apagu[eé]|prend[ií])|encendid\w*|apagad\w*|prendid\w*|sub[ií]|baj[eé]|puse|qued[oó]|al \d+|en \d+)\b", re.I)
_NEGACION = re.compile(r"\b(no|sigue|siguen|falló|fallo|error|inténtalo|intenta|pude|respond\w*|confirm\w*)\b", re.I)
SIN_JINX_COMPLETA = "Eso necesita a la Jinx completa y ahora no está disponible; pídemelo en un rato."
_RAW_CALL = re.compile(r"^\s*([A-Za-z_]+)\s*(\{.*\})\s*$", re.S)


def raw_tool_call(content, names):
    """Los modelos pequeños a veces escriben la llamada como TEXTO (`ha_buscar{"consulta": "tele"}`) en vez de usar el formato de
    herramientas. Si el contenido es solo eso, con el nombre de una herramienta conocida y JSON válido, se trata como la llamada.
    Cualquier otra cosa (texto normal, JSON roto, herramienta desconocida) devuelve None."""
    m = _RAW_CALL.match(content or "")
    if not m or m.group(1) not in names:
        return None
    try:
        args = json.loads(m.group(2))
    except ValueError:
        return None
    if not isinstance(args, dict):
        return None
    return {"id": "raw-%s" % m.group(1), "type": "function", "function": {"name": m.group(1), "arguments": json.dumps(args)}}


def clean_reply(text):
    t = re.sub(r"[*_`#>]+", "", text or "")
    t = " ".join(t.split())
    return t if len(t) <= REPLY_MAX else t[: REPLY_MAX - 1] + "…"


class OpenAICompatLLM:
    """Un modelo por una API compatible con OpenAI (OpenRouter por defecto). Lanza LiteError."""

    def __init__(self, url, model, key_fn, opener=urllib.request.urlopen, extra=None, timeout=25, max_tokens=300):
        self.url, self.model, self.key_fn, self.opener = url, model, key_fn, opener
        self.extra, self.timeout, self.max_tokens = extra or {}, timeout, max_tokens

    def chat(self, messages, tools):
        key = self.key_fn()
        if not key:
            raise LiteError("sin llave del modelo rápido")
        body = {"model": self.model, "messages": messages, "tools": tools, "temperature": 0.4, "max_tokens": self.max_tokens, **self.extra}
        req = urllib.request.Request(self.url, data=json.dumps(body).encode(), headers={
            "Content-Type": "application/json", "Authorization": "Bearer " + key})
        try:
            with self.opener(req, timeout=self.timeout) as r:
                return json.loads(r.read().decode("utf-8", "replace"))["choices"][0]["message"]
        except Exception as e:  # noqa: BLE001
            raise LiteError("el modelo rápido falló (%s)" % type(e).__name__) from None


def _compact_for_local(messages, prior=4):
    """Para el modelo local: persona mínima + hasta `prior` mensajes de texto previos + el turno actual completo (desde el último
    mensaje del usuario, con sus llamadas y resultados de herramientas, que no se pueden cortar a la mitad)."""
    last_user = max((i for i, m in enumerate(messages) if m.get("role") == "user"), default=len(messages) - 1)
    before = [m for m in messages[1:last_user]
              if m.get("role") in ("user", "assistant") and not m.get("tool_calls") and isinstance(m.get("content"), str)]
    ahora = next((l for l in str(messages[0].get("content", "")).splitlines() if l.startswith("Ahora:")), "") if messages else ""
    return [{"role": "system", "content": PERSONA_LOCAL + ("\n" + ahora if ahora else "")}] + before[-prior:] + messages[last_user:]


class FallbackLLM:
    """El modelo principal (OpenRouter) y, si falla —sin créditos, sin red, error—, el local. Tras un fallo del principal se salta durante
    `cooldown` segundos para no perder un segundo por turno esperando algo que ya sabemos que no responde."""

    def __init__(self, primary, local, cooldown=120, now=time.time, log=None):
        self.primary, self.local, self.cooldown, self.now = primary, local, cooldown, now
        self.log = log or (lambda m: None)
        self.down_until = 0.0
        self.last_fail = 0.0
        self.last_via = None

    def degraded(self):
        """True mientras el principal está caído: lo que necesite a la Jinx completa (que también usa OpenRouter) no va a salir."""
        return self.now() < self.down_until

    def recently_failed(self, window=1800):
        """¿Falló el principal hace poco? Mientras sí, conviene tener el modelo local cargado."""
        return self.last_fail > 0 and self.now() - self.last_fail < window

    def chat(self, messages, tools):
        if not self.degraded():
            try:
                m = self.primary.chat(messages, tools)
                self.last_via = "principal"
                return m
            except LiteError as e:
                self.down_until = self.now() + self.cooldown
                self.last_fail = self.now()
                self.log("modelo principal falló (%s): uso el local %ds" % (e, self.cooldown))
        m = self.local.chat(_compact_for_local(messages), tools)      # si también falla, LiteError sube (Jobs cae a la Jinx completa)
        self.last_via = "local"
        return m


SERVICE_TIMEOUT = 20      # segundos que se espera a que Home Assistant dé por hecha una orden
VERIFY_TRIES = 10       # veces que se mira si la orden hizo efecto
VERIFY_EVERY = 0.6      # segundos entre miradas: se confirma en cuanto cambia, sin esperar de más (seis segundos en total)


class Lite:
    def __init__(self, llm, ha, handoff, persona=PERSONA, now=time.time, sleep=time.sleep):
        self.llm, self.ha, self.handoff, self.persona, self.now = llm, ha, handoff, persona, now
        self.sleep = sleep
        self.history = []
        self.last = 0.0

    def _level(self, entity_id):
        """El volumen que dice el aparato (0..1), o None si no lo dice."""
        try:
            v = self.ha.state(entity_id).get("volume_level")
            return float(v) if v is not None else None
        except Exception:  # noqa: BLE001
            return None

    def _verified_volume(self, result, service, entity_id, before, data):
        """Subir, bajar o fijar el volumen solo está hecho cuando el aparato muestra otro volumen."""
        if before is None:
            return result
        target = (data or {}).get("volume_level")
        now = before
        for _ in range(VERIFY_TRIES):
            self.sleep(VERIFY_EVERY)
            now = self._level(entity_id)
            if now is None:
                return result
            if service == "volume_set" and target is not None and abs(now - float(target)) <= 0.006:  # una tele sube de 1 en 1: dos puntos de diferencia ya es otro volumen
                break
            if service == "volume_up" and now > before + 0.001:
                break
            if service == "volume_down" and now < before - 0.001:
                break
        else:
            return ("la orden se envió pero el volumen de %s sigue en %d%%. NO digas que cambió: dile que no ha respondido "
                    "o inténtalo de nuevo." % (entity_id, round(now * 100)))
        return "%s; el volumen de %s ahora es %d%%" % (result, entity_id, round(now * 100))

    def _verified(self, result, service, entity_id):
        """Encender o apagar solo está hecho cuando el aparato lo dice: Home Assistant contesta "ok" aunque la tele
        no se entere. Sin esto el modelo anunciaba "ya está encendida" con la tele apagada."""
        if service not in ("turn_on", "turn_off"):
            return result
        state = None
        for i in range(VERIFY_TRIES):
            self.sleep(VERIFY_EVERY)
            try:
                state = self.ha.state(entity_id).get("state")
            except Exception:  # noqa: BLE001 - si no se puede mirar, se dice tal cual
                return result
            if service == "turn_on" and state not in ("off", "unavailable", "unknown"):
                return "%s; %s ahora está «%s»" % (result, entity_id, state)
            if service == "turn_off" and state in ("off", "standby"):
                return "%s; %s ahora está «%s»" % (result, entity_id, state)
        return ("la orden se envió pero %s sigue «%s». NO digas que funcionó: si hay otro aparato del mismo tipo "
                "(busca con otra palabra, p. ej. parte de su entity_id) prueba con ese; si no, di que no respondió."
                % (entity_id, state))

    def _tool(self, name, args):
        try:
            if name == "ha_buscar":
                found = self.ha.find(str(args.get("consulta", "")), args.get("dominio"))
                if not found:
                    return ("sin resultados: en la casa no hay ningún aparato con ese nombre. Díselo a Alan tal cual y pregúntale cuál es; "
                            "NO supongas su estado ni lo confundas con otro aparato.")
                return json.dumps(found, ensure_ascii=False)
            if name == "ha_estado":
                return json.dumps(self.ha.state(args.get("entity_id")), ensure_ascii=False)
            if name == "ha_accion":
                service, eid, data = args.get("servicio"), args.get("entity_id"), args.get("datos")
                volume = service in ("volume_up", "volume_down", "volume_set")
                before = self._level(eid) if volume else None
                try:
                    result = self.ha.call(args.get("dominio"), service, eid, data)
                except HaTimeout:
                    # La orden pudo salir: se mira el estado antes de decir nada (encender una tele tarda en confirmarse).
                    result = "orden enviada (Home Assistant tardó en confirmarla)"
                    if args.get("servicio") not in ("turn_on", "turn_off"):
                        return result + "; no sé si hizo efecto"
                if volume:
                    return self._verified_volume(result, service, eid, before, data)
                return self._verified(result, service, eid)
            if name == "pasar_a_jinx":
                if getattr(self.llm, "degraded", lambda: False)():
                    return ("la Jinx completa no está disponible ahora (sin OpenRouter): dile a Alan que eso lo intente en un rato; "
                            "NO digas que lo pasaste")
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
        msgs = [{"role": "system", "content": self.persona + "\n" + ahora_texto(self.now())}] + self.history + [{"role": "user", "content": text}]
        out = ""
        failed = []        # acciones de la casa que no se pudieron confirmar: la respuesta no puede decir que se hicieron
        for _ in range(MAX_ROUNDS):
            m = self.llm.chat(msgs, TOOLS)
            calls = m.get("tool_calls") or []
            if not calls:
                raw = raw_tool_call(m.get("content"), {t["function"]["name"] for t in TOOLS})
                if raw:
                    m, calls = {"content": ""}, [raw]
            if not calls:
                if _TOOL_WORDS.match(m.get("content") or ""):
                    # El modelo escribió el nombre de una herramienta como si fuera la respuesta ("Pasar a jinx: anotar…"): eso no se le dice a Alan.
                    if getattr(self.llm, "degraded", lambda: False)():
                        out = SIN_JINX_COMPLETA
                        break
                    raise LiteError("el modelo escribió una herramienta como respuesta")
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
                if fn.get("name") == "ha_accion" and ("NO digas que" in result or result.startswith("error") or "no sé si hizo efecto" in result):
                    failed.append(result)
                msgs.append({"role": "tool", "tool_call_id": c.get("id", ""), "content": result[:TOOL_RESULT_MAX]})
        if not out:
            raise LiteError("el modelo no llegó a una respuesta")
        if failed and _EXITO.search(out) and not _NEGACION.search(out):
            out = NO_CONFIRMADA                      # el modelo (sobre todo el local) decía "listo" aunque la orden no surtió efecto
        self.history = (self.history + [{"role": "user", "content": text}, {"role": "assistant", "content": out}]
                        )[-HISTORY_TURNS:]
        return out
