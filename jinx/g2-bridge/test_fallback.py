"""Pruebas del respaldo local de la Jinx rápida (sin red). Ejecutar: python3 test_fallback.py"""
import json, g2_lite as L, g2_bridge as B

class Prim:
    def __init__(self, fail): self.fail, self.n = fail, 0
    def chat(self, messages, tools):
        self.n += 1
        if self.fail: raise L.LiteError("el modelo rápido falló (HTTPError)")
        return {"content": "Hola desde OpenRouter"}
class Loc:
    def __init__(self): self.seen = []
    def chat(self, messages, tools):
        self.seen.append(messages); return {"content": "Hola desde local"}

t = [1000.0]
now = lambda: t[0]
# 1) principal bien -> no toca el local
p, l = Prim(False), Loc(); f = L.FallbackLLM(p, l, now=now)
assert f.chat([{"role": "system", "content": "P"}, {"role": "user", "content": "x"}], [])["content"] == "Hola desde OpenRouter" and not l.seen and f.last_via == "principal"
# 2) principal falla -> local con persona mínima; el principal se salta durante el enfriamiento
p, l = Prim(True), Loc(); f = L.FallbackLLM(p, l, cooldown=120, now=now)
hist = [{"role": "system", "content": "PERSONA LARGA"}] + [{"role": "user" if i % 2 == 0 else "assistant", "content": "m%d" % i} for i in range(8)] + [{"role": "user", "content": "apaga la tele"}]
assert f.chat(hist, [])["content"] == "Hola desde local" and f.last_via == "local" and f.degraded()
m = l.seen[0]; assert m[0]["content"] == L.PERSONA_LOCAL and "LARGA" not in json.dumps(m) and m[-1]["content"] == "apaga la tele" and len(m) <= 1 + 4 + 1, m
f.chat(hist, []); assert p.n == 1                        # no volvió a intentar OpenRouter
t[0] += 121; f.chat(hist, []); assert p.n == 2           # pasado el enfriamiento, reintenta
# 3) no corta un turno con herramientas: llamada del asistente + resultado se conservan juntos
turn = [{"role": "system", "content": "P"}, {"role": "user", "content": "a"}, {"role": "assistant", "content": "b"}, {"role": "user", "content": "luz"},
        {"role": "assistant", "content": "", "tool_calls": [{"id": "1"}]}, {"role": "tool", "tool_call_id": "1", "content": "ok"}]
c = L._compact_for_local(turn); assert c[-2]["tool_calls"] and c[-1]["role"] == "tool" and c[1]["content"] == "a"
# 4) si los dos fallan, sube LiteError (Jobs cae a la Jinx completa)
class Loc2:
    def chat(self, m, t): raise L.LiteError("local caído")
try:
    L.FallbackLLM(Prim(True), Loc2(), now=now).chat(hist, []); raise SystemExit("MAL: no subió el error")
except L.LiteError: pass
# 5) pasar_a_jinx con OpenRouter caído no encola nada
class H:  # HA falso
    pass
calls = []
lite = L.Lite(L.FallbackLLM(Prim(True), Loc(), now=now), H(), lambda x: calls.append(x) or "pasado")
lite.llm.chat([{"role": "system", "content": "x"}, {"role": "user", "content": "y"}], [])     # abre el modo degradado
assert "no está disponible" in lite._tool("pasar_a_jinx", {"peticion": "agenda mañana"}) and not calls
lite2 = L.Lite(L.FallbackLLM(Prim(False), Loc(), now=now), H(), lambda x: calls.append(x) or "pasado")
assert lite2._tool("pasar_a_jinx", {"peticion": "agenda"}) == "pasado" and calls == ["agenda"]
# 6) el texto de error del gateway no pasa por respuesta de Jinx
assert B.es_error_llm("OpenRouter didn't answer after 4 attempts. Provider said: HTTP 402") and not B.es_error_llm("Listo, apagué la tele.")
# 7) llamada escrita como texto: se recupera solo si es una herramienta conocida con JSON válido
names = {"ha_buscar", "pasar_a_jinx"}
r = L.raw_tool_call('pasar_a_jinx{"peticion": "agenda mañana"}', names)
assert r and r["function"]["name"] == "pasar_a_jinx" and json.loads(r["function"]["arguments"]) == {"peticion": "agenda mañana"}
assert L.raw_tool_call("Hola, ¿qué tal?", names) is None and L.raw_tool_call('borrar_todo{"x": 1}', names) is None
assert L.raw_tool_call('ha_buscar{consulta:<|"|tele<|"|}', names) is None and L.raw_tool_call('ha_buscar{"a": 1} y algo más', names) is None
class Seq:                                     # el modelo escribe la llamada como texto y luego contesta con el resultado
    def __init__(self): self.n = 0
    def chat(self, messages, tools):
        self.n += 1
        return {"content": 'pasar_a_jinx{"peticion": "agenda"}'} if self.n == 1 else {"content": "Se lo pasé a Jinx."}
calls.clear(); lt = L.Lite(Seq(), H(), lambda x: calls.append(x) or "pasado")
assert lt.reply("anota mi cita") == "Se lo pasé a Jinx." and calls == ["agenda"]
print("OK test_fallback")

# 8) búsqueda de dispositivos con los nombres reales de la casa
STATES = [
    {"entity_id": "sensor.55_oled_canal_de_tv", "state": "x", "attributes": {"friendly_name": "55 OLED Canal de TV"}},
    {"entity_id": "media_player.55_oled_qn55s90fafxzx", "state": "on", "attributes": {"friendly_name": "Tv habitacion"}},
    {"entity_id": "media_player.intv_1000", "state": "paused", "attributes": {"friendly_name": "INTV 1000"}},
    {"entity_id": "switch.light_sensor_55_oled", "state": "off", "attributes": {"friendly_name": "Light Sensor - 55\" OLED"}},
    {"entity_id": "switch.pi_hole", "state": "off", "attributes": {"friendly_name": "Pi-Hole"}},
]
class FakeHA(L.HomeAssistant):
    def __init__(self): pass
    def states(self): return STATES
ha = FakeHA()
ids = lambda q: [o["entity_id"] for o in ha.find(q)]
assert ids("tele del cuarto")[0] == "media_player.55_oled_qn55s90fafxzx", ids("tele del cuarto")
assert ids("la tele de la habitación") == ["media_player.55_oled_qn55s90fafxzx"]
assert ids("tele")[0].startswith("media_player.") and ids("tele").index("sensor.55_oled_canal_de_tv") > 0      # el aparato antes que el sensor
assert "media_player.intv_1000" not in ids("tele")                                                           # "INTV 1000" no es una tele
assert ids("tele de la sala") == [] and ids("tele cocina") == []                                               # una palabra de más NO convierte otra habitación en esta
assert ids("pi hole") == ["switch.pi_hole"]
assert L.ahora_texto(0).startswith("Ahora: ") and "de " in L.ahora_texto(1793928277)
m = L._compact_for_local([{"role": "system", "content": "P\nAhora: jueves 5 de noviembre de 2026, 21:45."}, {"role": "user", "content": "hora"}])
assert "Ahora: jueves" in m[0]["content"] and m[0]["content"].startswith(L.PERSONA_LOCAL)
print("OK find+hora")

# 9) una herramienta escrita como respuesta no se entrega; con OpenRouter caído se explica, con OpenRouter bien sube el error (la completa contesta)
class Txt:
    def chat(self, messages, tools): return {"content": "Pasar a jinx: anotar cita dentista mañana a las 5."}
down = L.FallbackLLM(Prim(True), Txt(), now=now)
lt = L.Lite(down, H(), lambda x: "pasado"); assert lt.reply("anota mi cita") == L.SIN_JINX_COMPLETA
up = L.FallbackLLM(Prim(False), Txt(), now=now); up.primary.chat = lambda m, t: {"content": "pasar_a_jinx: anotar"}
try:
    L.Lite(up, H(), lambda x: "pasado").reply("anota"); raise SystemExit("MAL")
except L.LiteError: pass
assert down.recently_failed() and not L.FallbackLLM(Prim(False), Txt(), now=now).recently_failed()
print("OK herramienta-como-texto + warm")

# 10) acción de la casa sin confirmar: no se puede contestar "listo"
class Acc:
    def __init__(self, final): self.n, self.final = 0, final
    def chat(self, messages, tools):
        self.n += 1
        if self.n == 1:
            return {"content": "", "tool_calls": [{"id": "1", "type": "function", "function": {"name": "ha_accion", "arguments": json.dumps({"dominio": "media_player", "servicio": "volume_set", "entity_id": "media_player.x", "datos": {"volume_level": 0.2}})}}]}
        return {"content": self.final}
class HAmal:
    def call(self, *a, **k): return "ok"
    def state(self, e): return {"volume_level": 0.09}
lt = L.Lite(Acc("Listo, el volumen está en 20."), HAmal(), lambda x: "p", sleep=lambda s: None); lt._verified_volume = lambda *a, **k: "la orden se envió pero el volumen sigue en 9%. NO digas que cambió"
assert lt.reply("sube el volumen") == L.NO_CONFIRMADA
lt = L.Lite(Acc("No cambió, la tele no respondió."), HAmal(), lambda x: "p", sleep=lambda s: None); lt._verified_volume = lambda *a, **k: "la orden se envió pero el volumen sigue en 9%. NO digas que cambió"
assert lt.reply("sube el volumen") == "No cambió, la tele no respondió."
print("OK accion-no-confirmada")

# 11) una respuesta honesta ("a mano") no se reemplaza aunque la acción haya fallado
lt = L.Lite(Acc("La cerradura la abres tú a mano."), HAmal(), lambda x: "p", sleep=lambda s: None); lt._verified_volume = lambda *a, **k: "NO digas que cambió"
assert lt.reply("abre la puerta") == "La cerradura la abres tú a mano."
print("OK sin-relajar+honesta")
