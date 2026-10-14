#!/usr/bin/env python3
"""Gateway de pendientes de Jinx para la app Coucou del teléfono (comando forzado por ssh).

Corre EN EL SERVER (`~/.hermes/scripts/coucou_pendientes.py`, con el python del venv de Hermes). Lee UNA línea
JSON de stdin y escribe UNA línea JSON. Las llaves de los teléfonos solo pueden ejecutar esto (authorized_keys
con command=, restrict): no hay shell ni argumentos del cliente en ninguna línea de comandos.

  {"op":"list"}                                   -> {"ok":true,"items":[{id,titulo,due,categoria,source,status,updated_at}…]}
  {"op":"add","titulo","due","nota","source_id"}  -> {"ok":true,"id":"…"}
  {"op":"resolve","id","accion"}                  -> {"ok":true}   accion: hecho | descartado | reabrir

Los cambios pasan por el propio módulo de pendientes de Jinx (`pendientes_store`), nunca se edita su archivo.
"""
import json
import os
import re
import sys
import time

FIELDS = ("id", "titulo", "due", "categoria", "source", "status", "updated_at")
ACTIONS = ("hecho", "descartado", "reabrir")
SAFE_ID = re.compile(r"^[A-Za-z0-9_.:-]{1,80}$")
DATE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
RECENT_DAYS = 30


def _err(msg):
    return {"ok": False, "error": msg}


def _age_days(updated_at, now_iso):
    """Días entre dos ISO (aprox.); si no se puede leer, 0 (se conserva)."""
    try:
        t = time.mktime(time.strptime(str(updated_at)[:19], "%Y-%m-%dT%H:%M:%S"))
        n = time.mktime(time.strptime(now_iso[:19], "%Y-%m-%dT%H:%M:%S"))
        return (n - t) / 86400.0
    except (ValueError, TypeError):
        return 0.0


def handle(req, store, now_iso=None):
    if not isinstance(req, dict):
        return _err("petición no válida")
    now_iso = now_iso or time.strftime("%Y-%m-%dT%H:%M:%S")
    op = req.get("op")
    try:
        if op == "list":
            out = []
            for it in store.load():
                is_open = it.get("status") == "pendiente"
                if not is_open and _age_days(it.get("updated_at"), now_iso) > RECENT_DAYS:
                    continue   # un pendiente cerrado hace mucho ya no cambia nada en el teléfono
                out.append({k: it.get(k) for k in FIELDS})
            return {"ok": True, "items": out}
        if op == "add":
            titulo = req.get("titulo")
            source_id = req.get("source_id")
            due = req.get("due")
            nota = req.get("nota", "")
            if not isinstance(titulo, str) or not titulo.strip() or len(titulo) > 200:
                return _err("título no válido")
            if not isinstance(source_id, str) or not SAFE_ID.match(source_id):
                return _err("source_id no válido")
            if due is not None and not (isinstance(due, str) and DATE.match(due)):
                return _err("fecha no válida")
            if not isinstance(nota, str) or len(nota) > 200:
                return _err("nota no válida")
            res = store.add(titulo.strip(), due=due, categoria="personal", source="coucou",
                            source_id=source_id, nota=nota)
            if isinstance(res, dict) and res.get("error"):
                return _err(str(res["error"])[:120])
            return {"ok": True, "id": res.get("id")}
        if op == "resolve":
            ref, accion = req.get("id"), req.get("accion")
            if not isinstance(ref, str) or not SAFE_ID.match(ref):
                return _err("id no válido")
            if accion not in ACTIONS:
                return _err("acción no válida")
            res = store.resolve(ref, accion)
            if isinstance(res, dict) and res.get("error"):
                return _err(str(res["error"])[:120])
            return {"ok": True}
        return _err("op desconocida")
    except Exception as e:  # noqa: BLE001 - el cliente solo necesita saber que falló
        return _err("falló el almacén: %s" % type(e).__name__)


def main():
    raw = sys.stdin.readline()
    try:
        req = json.loads(raw) if raw.strip() else {}
    except ValueError:
        print(json.dumps(_err("petición no es JSON")))
        return 0
    sys.path.insert(0, os.path.expanduser("~/.hermes/plugins/pendientes"))
    try:
        import pendientes_store as store  # type: ignore
    except Exception as e:  # noqa: BLE001
        print(json.dumps(_err("no pude abrir el almacén: %s" % type(e).__name__)))
        return 0
    print(json.dumps(handle(req, store), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
