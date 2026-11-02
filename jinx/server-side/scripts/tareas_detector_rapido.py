#!/usr/bin/env python3
"""tareas_detector_rapido.py — detector de tareas nuevas de Teams.

Corre cada 10-15 min. Escanea Teams y notifica a Alan cuando aparece una tarea nueva.

Uso:
    python3 tareas_detector_rapido.py           # tick() normal
    python3 tareas_detector_rapido.py --dry-run # print en vez de notify
    python3 tareas_detector_rapido.py --self-check  # test básico
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
from datetime import date, datetime
from pathlib import Path
from typing import Any

# Configuración
_HERMES_HOME = Path(os.environ.get("HERMES_HOME", str(Path.home() / ".hermes")))
STATE_FILE = _HERMES_HOME / "state" / "tareas_detector_rapido.json"
NOTIFY_SCRIPT = _HERMES_HOME / "scripts" / "notify_alan.sh"

# Cargar el plugin teams-tareas
_TEAMSTAREAS_PATH = _HERMES_HOME / "plugins" / "teams-tareas" / "__init__.py"
if not _TEAMSTAREAS_PATH.exists():
    raise RuntimeError(f"teams-tareas no encontrado en {_TEAMSTAREAS_PATH}")

spec = importlib.util.spec_from_file_location("teams_tareas", _TEAMSTAREAS_PATH)
if spec is None or spec.loader is None:
    raise RuntimeError(f"No se pudo cargar teams-tareas desde {_TEAMSTAREAS_PATH}")
tt = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tt)


def _now_iso() -> str:
    return datetime.now().isoformat()


def _load_known_ids() -> set[str]:
    """Carga los assignment_ids ya conocidos desde el estado."""
    try:
        data = json.loads(STATE_FILE.read_text(encoding="utf-8"))
        return set(data.get("known_ids", []))
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return set()


def _save_known_ids(ids: set[str]) -> None:
    """Guarda los assignment_ids conocidos al estado."""
    STATE_FILE.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=str(STATE_FILE.parent), prefix=".tdr_", suffix=".tmp")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump({"known_ids": list(ids), "updated_at": _now_iso()}, f, ensure_ascii=False)
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp, STATE_FILE)
        os.chmod(STATE_FILE, 0o600)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


def _notify(mensaje: str, dry_run: bool = False) -> bool:
    """Envía notificación a Alan via notify_alan.sh."""
    if dry_run:
        print(f"[DRY-RUN] Notificación: {mensaje}")
        return True

    try:
        result = subprocess.run(
            ["bash", str(NOTIFY_SCRIPT), mensaje],
            capture_output=True,
            text=True,
            timeout=90
        )
        if result.returncode == 0:
            print(f"Notificación enviada: {result.stdout.strip()}")
            return True
        else:
            print(f"Error notify: {result.stderr.strip()}")
            return False
    except subprocess.TimeoutExpired:
        print("Timeout en notify_alan.sh")
        return False
    except Exception as e:
        print(f"Excepción en notify: {e}")
        return False


# Mismo centinela que tareas_teams_check.py: un solo aviso por caida de sesion, se borra al volver a escanear bien.
REAUTH_SENTINEL = _HERMES_HOME / "state" / "teams_tareas_reauth_avisado"


def _avisar_reauth(exc: Exception, dry_run: bool = False) -> None:
    """Sin esto la sesion de Teams caducaba en silencio: el detector solo anotaba el error en su resultado."""
    txt = str(exc)
    if not ("invalid_grant" in txt or "AADSTS" in txt or type(exc).__name__ == "ReauthNeeded"):
        return
    if REAUTH_SENTINEL.exists():
        return
    if _notify("🔑 Perdí la sesión con tu Teams, así que no veo tareas nuevas. Corre en el server:\n"
               "  ~/.hermes/scripts/tareas_teams_auth.py\ny sigue el código.", dry_run=dry_run) and not dry_run:
        REAUTH_SENTINEL.parent.mkdir(parents=True, exist_ok=True)
        REAUTH_SENTINEL.write_text(_now_iso())


def _limpiar_reauth(dry_run: bool = False) -> None:
    if not dry_run and REAUTH_SENTINEL.exists():
        REAUTH_SENTINEL.unlink()


def _es_webinar(instrucciones: str) -> bool:
    """Detecta si las instrucciones contienen link de YouTube (webinar)."""
    if not instrucciones:
        return False
    lower = instrucciones.lower()
    return "youtube.com" in lower or "youtu.be" in lower


def _formatea_tarea(tarea: dict, detalle: dict | None = None) -> str:
    """Formatea una tarea para el mensaje de notificación."""
    clase = tarea.get("clase", "Sin clase")
    titulo = tarea.get("titulo", "Sin título")
    vence = tarea.get("vence")
    dias = tarea.get("dias")

    # Formatear vencimiento
    if vence:
        try:
            dt = datetime.fromisoformat(vence.replace("Z", "+00:00"))
            vence_str = dt.strftime("%d/%m %H:%M")
        except Exception:
            vence_str = str(vence)[:10]
    else:
        vence_str = "Sin fecha"

    # Días restantes
    if dias is not None:
        if dias < 0:
            dias_str = f"venció hace {abs(dias)} día(s)"
        elif dias == 0:
            dias_str = "vence HOY"
        elif dias == 1:
            dias_str = "vence mañana"
        else:
            dias_str = f"vence en {dias} días"
    else:
        dias_str = ""

    # Detectar webinar
    webinar = ""
    if detalle and _es_webinar(detalle.get("instrucciones", "")):
        webinar = " [WEBINAR]"

    return f"{clase}: {titulo} — {vence_str} ({dias_str}){webinar}"


def tick(*, dry_run: bool = False) -> dict:
    """
    Escanea Teams y detecta tareas nuevas.
    Retorna dict con: nuevas_encontradas, notificadas, errores.
    """
    resultado = {
        "nuevas_encontradas": 0,
        "notificadas": 0,
        "errores": [],
    }

    # Cargar IDs conocidos
    known_ids = _load_known_ids()

    # Refrescar estado desde Teams
    try:
        res = tt.refrescar_estado()
    except Exception as e:
        resultado["errores"].append(f"refrescar_estado: {e}")
        _avisar_reauth(e, dry_run)
        return resultado
    _limpiar_reauth(dry_run)

    # Las tareas nuevas vienen en res["nuevas"] (lista de dicts)
    nuevas = res.get("nuevas", [])

    if not nuevas:
        print("No hay tareas nuevas.")
        # Actualizar known_ids con todas las tareas vistas
        todas = tt.load()
        todos_ids = {t.get("assignment_id") or t.get("id") for t in todas if t.get("assignment_id") or t.get("id")}
        if todos_ids != known_ids:
            _save_known_ids(todos_ids)
            print(f"Actualizados known_ids: {len(todos_ids)} tareas")
        return resultado

    print(f"Encontradas {len(nuevas)} tareas nuevas")

    # Detectar cuáles son realmente nuevas (no en known_ids)
    truly_new = []
    for t in nuevas:
        aid = t.get("assignment_id") or t.get("id")
        if aid and aid not in known_ids:
            truly_new.append((aid, t))

    if not truly_new:
        print("Todas las tareas nuevas ya estaban en known_ids.")
        return resultado

    resultado["nuevas_encontradas"] = len(truly_new)

    # Procesar cada tarea nueva
    for aid, tarea in truly_new:
        # Obtener detalle para ver si es webinar
        detalle = None
        try:
            detalle_raw = tt.tarea_detalle({"ref": aid})
            detalle = json.loads(detalle_raw) if detalle_raw else None
        except Exception as e:
            print(f"Error obteniendo detalle de {aid}: {e}")

        # Formatear mensaje
        msg = _formatea_tarea(tarea, detalle)

        # Notificar
        if _notify(msg, dry_run=dry_run):
            resultado["notificadas"] += 1

        # Borrador de texto en segundo plano (no bloquea el escaneo; nunca entrega nada). Lo omite en dry-run.
        if not dry_run and aid:
            try:
                subprocess.Popen([sys.executable, str(_HERMES_HOME / "scripts" / "borrador_tarea.py"), aid],
                                 stdout=open(_HERMES_HOME / "logs" / "borrador_tarea.log", "a"),
                                 stderr=subprocess.STDOUT, start_new_session=True)
            except Exception as e:  # noqa: BLE001
                print(f"No pude lanzar el borrador de {aid}: {e}")

    # Actualizar known_ids con las tareas nuevas
    for aid, _ in truly_new:
        known_ids.add(aid)
    if truly_new:
        _save_known_ids(known_ids)

    return resultado


def self_check() -> int:
    """Test básico de funcionamiento."""
    print("Ejecutando self-check...")

    # Verificar que teams-tareas se cargó
    if not hasattr(tt, "refrescar_estado"):
        print("FALLO: tt.refrescar_estado no existe")
        return 1

    if not hasattr(tt, "tarea_detalle"):
        print("FALLO: tt.tarea_detalle no existe")
        return 1

    # Verificar archivo de estado
    print(f"Estado file: {STATE_FILE}")
    known = _load_known_ids()
    print(f"Known IDs cargados: {len(known)}")

    # Verificar notify script
    if NOTIFY_SCRIPT.exists():
        print(f"Notify script encontrado: {NOTIFY_SCRIPT}")
    else:
        print(f"WARNING: notify script no encontrado en {NOTIFY_SCRIPT}")

    # Test de detección de webinar
    assert _es_webinar("Ver el video en https://youtube.com/watch?v=abc") == True
    assert _es_webinar("https://youtu.be/xyz") == True
    assert _es_webinar("https://example.com") == False
    assert _es_webinar("") == False
    print("Test _es_webinar: OK")

    print("Self-check completado exitosamente.")
    return 0


def main():
    parser = argparse.ArgumentParser(description="Detector rápido de tareas de Teams")
    parser.add_argument("--dry-run", action="store_true",
                        help="Imprime notificaciones en vez de enviarlas")
    parser.add_argument("--self-check", action="store_true",
                        help="Ejecuta test básico de funcionamiento")

    args = parser.parse_args()

    if args.self_check:
        sys.exit(self_check())

    resultado = tick(dry_run=args.dry_run)
    print(f"Resultado: {resultado}")


if __name__ == "__main__":
    main()
