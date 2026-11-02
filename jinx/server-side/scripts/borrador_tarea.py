#!/usr/bin/env python3
"""borrador_tarea.py — deja un BORRADOR de texto para una tarea de Teams y avisa a Alan.

Lo llama tareas_detector_rapido.py cuando aparece una tarea nueva, o a mano:
    python3 borrador_tarea.py <assignment_id> [--force]

Hace lo que la tarea pide, o la parte que se puede hacer desde aqui: lee las imagenes adjuntas, redacta el contenido
final y busca fuentes reales en la web. Nunca entrega nada (tarea_entregar sigue pidiendo el visto bueno de Alan) y
no registra asistencia a nada. Si dos materias piden lo mismo (mismas instrucciones y adjuntos), el trabajo se hace una
sola vez. Los webinars no se redactan: su resumen depende de haber visto la sesion.
"""
from __future__ import annotations

import importlib.util
import json
import os
import re
import subprocess
import sys
import unicodedata
import urllib.error
import urllib.request
from datetime import datetime
from pathlib import Path

HERMES = Path(os.environ.get("HERMES_HOME", str(Path.home() / ".hermes")))
OUT_DIR = HERMES / "state" / "borradores"
NOTIFY = HERMES / "scripts" / "notify_alan.sh"
INDEX = OUT_DIR / "index.json"   # huella del contenido -> borrador ya hecho
MAX_IMG = 4
MODELO_VISION = "google/gemini-2.5-flash"
MODELO_TEXTO = "deepseek/deepseek-v4-flash"  # el de config.yaml
TELEGRAM_CAP = 3800


def _load(path: Path, name: str):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    sys.path.insert(0, str(path.parent))
    spec.loader.exec_module(mod)
    return mod


def _slug(s: str) -> str:
    s = unicodedata.normalize("NFKD", s).encode("ascii", "ignore").decode()
    return re.sub(r"[^a-z0-9]+", "-", s.lower()).strip("-")[:40] or "tarea"


def _prompt(d: dict, adjuntos: list[str], sin_leer: list[str]) -> str:
    extra = ""
    if adjuntos:
        extra += f"\nTe adjunto {len(adjuntos)} imagen(es) de la tarea ({', '.join(adjuntos)}): leelas, ahi esta la actividad.\n"
    if sin_leer:
        extra += f"\nAdjuntos que NO pudiste leer: {', '.join(sin_leer)}. Marca lo que dependa de ellos como [ALAN: abrir {sin_leer[0]}].\n"
    return (
        "Eres un asistente de estudio. Alan es estudiante de preparatoria (Mexico). Haz lo que esta tarea de Teams pide, "
        "o la parte que se pueda hacer desde aqui, y dejalo listo para usar: contenido final, no un esquema. Espanol.\n\n"
        f"CLASE: {d.get('clase')}\nTAREA: {d.get('titulo')}\nVENCE: {d.get('vence')}\nPUNTOS: {d.get('puntos_max')}\n\n"
        f"INSTRUCCIONES:\n{d.get('instrucciones')}\n{extra}\n"
        "REGLAS:\n"
        "- Sigue las instrucciones y la actividad de la imagen al pie de la letra (formato, numero de partes, extension).\n"
        "- Si piden fuentes, busca en la web y cita SOLO paginas que realmente encontraste, con su URL, en formato APA al final. "
        "Nunca inventes una fuente, URL, cita ni cifra.\n"
        "- Redacta claro y breve, a nivel de preparatoria.\n"
        "- Solo lo que de verdad dependa de Alan (su nombre, una foto suya, una experiencia propia, un archivo que no puedes ver, "
        "algo que debe hacerse fuera, como una presentacion en Canva) va como [ALAN: que falta]. Todo lo demas, completo.\n"
        "- Si la tarea NO se puede avanzar desde texto (asistir a una sesion, un examen en linea), responde solo: NO_APLICA\n"
        "- Tablas markdown compactas: una linea por fila, celdas cortas, sin rellenar con espacios ni guiones de relleno.\n"
        "- Responde solo con el trabajo, sin introduccion ni comentarios."
    )


class _SinRedireccion(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *a, **k):
        return None


def _bajar_graph(url: str, tok: str) -> bytes:
    """El enlace viene de datos del profesor: el token solo viaja a graph.microsoft.com (https), y la redireccion a la
    URL de descarga (otro host, ya autenticada) se sigue SIN la cabecera Authorization."""
    from urllib.parse import urlparse
    u = urlparse(url)
    if u.scheme != "https" or (u.hostname or "").rstrip(".").lower() != "graph.microsoft.com":
        raise ValueError("adjunto fuera de graph.microsoft.com: no se envia el token")
    op = urllib.request.build_opener(_SinRedireccion)
    try:
        with op.open(urllib.request.Request(url, headers={"Authorization": "Bearer " + tok}), timeout=40) as x:
            return x.read()
    except urllib.error.HTTPError as e:
        loc = e.headers.get("Location") if e.code in (301, 302, 303, 307, 308) else None
        if not loc:
            raise
        d = urlparse(loc)
        if d.scheme != "https":
            raise ValueError("redireccion no https")
        with urllib.request.urlopen(urllib.request.Request(loc), timeout=40) as x:   # sin token
            return x.read()


def _imagenes(tt, d: dict) -> tuple[list[dict], list[str], list[str]]:
    """(partes image_url, nombres leidos, nombres sin leer) de los adjuntos del profesor."""
    import base64
    import urllib.request
    partes, leidos, sin_leer = [], [], []
    tok = None
    for r in d.get("recursos_profe") or []:
        nombre, link = r.get("nombre") or "adjunto", r.get("link") or ""
        ext = nombre.lower().rsplit(".", 1)[-1]
        if ext not in ("jpg", "jpeg", "png", "webp") or not link or len(partes) >= MAX_IMG:
            if link or nombre:
                sin_leer.append(nombre)
            continue
        try:
            tok = tok or tt.get_access_token()
            b = _bajar_graph(link + "/content", tok)
            mime = {"jpg": "jpeg", "jpeg": "jpeg", "png": "png", "webp": "webp"}[ext]
            partes.append({"type": "image_url", "image_url": {"url": f"data:image/{mime};base64,{base64.b64encode(b).decode()}"}})
            leidos.append(nombre)
        except Exception as e:  # noqa: BLE001
            print(f"[borrador] no pude bajar {nombre}: {str(e)[:80]}", file=sys.stderr)
            sin_leer.append(nombre)
    return partes, leidos, sin_leer


def _llamar(key: str, modelo: str, contenido, max_tokens: int, web: bool = False, temp: float = 0.4) -> str | None:
    import urllib.error
    import urllib.request
    body = {"model": modelo, "messages": [{"role": "user", "content": contenido}], "temperature": temp,
            "max_tokens": max_tokens, "reasoning": {"enabled": False}}
    if web:
        body["plugins"] = [{"id": "web", "max_results": 5}]
    req = urllib.request.Request("https://openrouter.ai/api/v1/chat/completions", data=json.dumps(body).encode(),
                                 method="POST", headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=200) as r:
            return (((json.loads(r.read()).get("choices") or [{}])[0].get("message") or {}).get("content") or "").strip() or None
    except (urllib.error.URLError, OSError, ValueError) as e:
        print(f"[borrador] {modelo} web={web}: {str(e)[:100]}", file=sys.stderr)
        return None


def generar(tt, d: dict) -> str | None:
    """OpenRouter directo (el agente de Hermes tiene la personalidad de Jinx y contesta conversando en vez de redactar).
    Paso 1: un modelo con vision transcribe lo que piden las imagenes. Paso 2: un modelo de texto, con plugin web para
    fuentes reales, hace el trabajo (los modelos con vision rellenaban las tablas con espacios hasta agotar los tokens)."""
    key = tt._secret("OPENROUTER_API_KEY") or ""
    if not key:
        print("[borrador] sin OPENROUTER_API_KEY", file=sys.stderr)
        return None
    partes, leidos, sin_leer = _imagenes(tt, d)
    if partes:
        vista = _llamar(key, MODELO_VISION, [{"type": "text", "text":
                "Transcribe con exactitud todo el texto de estas imagenes de una tarea escolar y describe en pocas lineas que "
                "hay que hacer (tablas, columnas, preguntas, numero de items). No resuelvas nada."}] + partes, 1800, temp=0)
        if vista:
            d = {**d, "instrucciones": f"{d.get('instrucciones')}\n\nCONTENIDO DE LA IMAGEN ADJUNTA ({', '.join(leidos)}):\n{vista}"}
        else:
            sin_leer = leidos + sin_leer
        leidos = []
    prompt = _prompt(d, [], sin_leer)
    return (_llamar(key, MODELO_TEXTO, prompt, 6000, web=True)
            or ((_llamar(key, MODELO_TEXTO, prompt, 6000) or "") + "\n\n[BUSCAR: fuentes reales; no se pudo buscar en la web]").strip() or None)


def _huella(d: dict) -> str:
    import hashlib
    base = re.sub(r"\s+", " ", (d.get("instrucciones") or "").lower()).strip()
    adj = ",".join(sorted((r.get("nombre") or "") for r in d.get("recursos_profe") or []))
    return hashlib.sha1(f"{base}|{adj}".encode()).hexdigest()[:16]


def _index() -> dict:
    try:
        return json.loads(INDEX.read_text())
    except (OSError, ValueError):
        return {}


def _md_a_docx(texto: str, titulo: str, ruta: Path) -> None:
    """Markdown sencillo (#, -, tablas con |, **negrita**) a .docx sin dependencias (no hay python-docx en el server)."""
    import zipfile
    from xml.sax.saxutils import escape

    def runs(t: str, bold_all: bool = False, size: int | None = None) -> str:
        out = []
        for k, trozo in enumerate(re.split(r"\*\*", t)):
            if not trozo:
                continue
            trozo = trozo.replace("*", "")
            rpr = ("<w:b/>" if (bold_all or k % 2) else "") + (f'<w:sz w:val="{size}"/>' if size else "")
            out.append(f'<w:r><w:rPr>{rpr}</w:rPr><w:t xml:space="preserve">{escape(trozo)}</w:t></w:r>')
        return "".join(out)

    def par(t: str, **kw) -> str:
        return '<w:p><w:pPr><w:spacing w:after="120"/></w:pPr>' + runs(t, **kw) + "</w:p>"

    def tabla(filas: list) -> str:
        borde = "".join(f'<w:{b} w:val="single" w:sz="4" w:color="808080"/>'
                        for b in ("top", "left", "bottom", "right", "insideH", "insideV"))
        x = ['<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblBorders>' + borde + "</w:tblBorders></w:tblPr>"]
        for n, f in enumerate(filas):
            x.append("<w:tr>" + "".join("<w:tc><w:tcPr/>" + par(c, bold_all=(n == 0)) + "</w:tc>" for c in f) + "</w:tr>")
        x.append("</w:tbl><w:p/>")
        return "".join(x)

    cuerpo = [par(titulo, bold_all=True, size=32)]
    fila_tabla: list = []

    def cierra() -> None:
        if fila_tabla:
            ancho = max(len(f) for f in fila_tabla)
            cuerpo.append(tabla([f + [""] * (ancho - len(f)) for f in fila_tabla]))
            fila_tabla.clear()

    for linea in texto.split("\n"):
        l = linea.strip()
        if l.startswith("|"):
            fila_tabla.append([c.strip() for c in l.strip("|").split("|")])
            continue
        cierra()
        if not l or l == "---":
            continue
        m = re.match(r"(#{1,6})\s+(.*)", l)
        if m:
            cuerpo.append(par(m.group(2), bold_all=True, size=28 if len(m.group(1)) <= 2 else 24))
        elif re.match(r"[-*•]\s+", l):
            cuerpo.append(par("• " + re.sub(r"^[-*•]\s+", "", l)))
        else:
            cuerpo.append(par(l))
    cierra()
    ns = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
    doc = f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {ns}><w:body>{"".join(cuerpo)}</w:body></w:document>'
    with zipfile.ZipFile(ruta, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>')
        z.writestr("_rels/.rels", '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>')
        z.writestr("word/document.xml", doc)


def _adjuntar(tt, aid: str, docx: Path) -> str:
    """Sube el .docx a la tarea EN BORRADOR (no entrega: tarea_entregar necesita el visto bueno de Alan)."""
    try:
        r = json.loads(tt.tarea_adjuntar({"ref": aid, "ruta_archivo": str(docx)}))
    except Exception as e:  # noqa: BLE001
        return f"fallo ({e})"
    return "adjuntado sin entregar" if r.get("ok") else f"fallo ({r.get('error')})"


PERFIL = "tareas"   # perfil de Hermes sin la personalidad de Jinx (~/.hermes/profiles/tareas)
HERMES_BIN = Path.home() / ".hermes" / "hermes-agent" / "venv" / "bin" / "hermes"
VENV_PY = HERMES / "profiles" / PERFIL / "venv" / "bin" / "python"
SALIDAS = (".docx", ".pptx", ".pdf", ".xlsx")


def _prompt_agente(d: dict, adjuntos: list[str], sin_leer: list[str]) -> str:
    extra = ""
    if adjuntos:
        extra += ("\nLas imagenes de la tarea estan en ./adjuntos/ (" + ", ".join(adjuntos) +
                  "): abrelas con tu herramienta de vision, ahi esta la actividad.\n")
    if sin_leer:
        extra += f"\nAdjuntos que no pudiste obtener: {', '.join(sin_leer)}. Marca lo que dependa de ellos como [ALAN: ...].\n"
    return (
        "Haz esta tarea escolar de Alan, o la parte que se pueda hacer desde aqui, y dejala como archivo(s) listos para usar "
        "en el directorio actual. Elige el formato que pide la tarea (documento .docx, presentacion .pptx, etc.; si pide Canva "
        "o algo que no puedes crear, haz el contenido en .pptx y dilo en una linea).\n\n"
        f"CLASE: {d.get('clase')}\nTAREA: {d.get('titulo')}\nVENCE: {d.get('vence')}\nPUNTOS: {d.get('puntos_max')}\n\n"
        f"INSTRUCCIONES:\n{d.get('instrucciones')}\n{extra}\n"
        f"Para crear archivos Office usa {VENV_PY} (trae python-docx y python-pptx). Termina con una linea 'ARCHIVO: <ruta>' por "
        "archivo creado, o solo NO_APLICA si no se puede avanzar desde texto o archivos."
    )


def _sandbox(work: Path, copia: Path) -> list[str] | None:
    """Comando bwrap para el agente: lee instrucciones del profesor y paginas web (texto no confiable) y tiene ejecucion de
    codigo, asi que corre sin acceso al resto del $HOME (ssh, .hermes/.env, otros perfiles), con su propia identidad
    (SOUL, config, skills, memoria) de solo lectura y escribiendo solo en su carpeta de trabajo. La red queda abierta
    (OpenRouter y busqueda web). None si no hay bwrap: sin aislamiento no se ejecuta el agente."""
    import shutil
    bwrap = shutil.which("bwrap")
    if not bwrap:
        return None
    home = str(Path.home())
    perfil = HERMES / "profiles" / PERFIL
    cmd = [bwrap, "--die-with-parent", "--new-session", "--cap-drop", "ALL", "--unshare-pid", "--unshare-ipc", "--unshare-uts",
           "--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc", "--tmpfs", "/tmp", "--tmpfs", home,
           # /run trae el bus de systemd del usuario y los sockets (docker, dbus): desde ahi se sale de la caja
           "--tmpfs", "/run"]
    for tapar in ("/mnt", "/media", "/srv", "/root", "/var/lib/docker"):
        if os.path.isdir(tapar):
            cmd += ["--tmpfs", tapar]
    # El perfil que ve el agente es una COPIA desechable (solo identidad: SOUL, config, skills, memoria), escribible
    # pero que se borra al terminar: lo que Hermes cree (plugins, hooks, .env...) no sobrevive a la corrida. Lo
    # que carga como identidad va ademas de solo lectura, y el venv es el original en solo lectura.
    cmd += ["--ro-bind", str(HERMES / "hermes-agent"), str(HERMES / "hermes-agent"),
            "--bind", str(copia), str(perfil)]
    for f in ("SOUL.md", "config.yaml", "profile.yaml", "skills", "memories"):
        if (copia / f).exists():
            cmd += ["--ro-bind", str(copia / f), str(perfil / f)]
    cmd += ["--ro-bind", str(perfil / "venv"), str(perfil / "venv")]
    cmd += ["--bind", str(work), str(work), "--chdir", str(work)]
    return cmd


def _agente(tt, d: dict, aid: str):
    """Corre el perfil `tareas` de Hermes (herramientas y skills, sin la personalidad de Jinx) en un directorio propio.
    Devuelve (carpeta, [archivos]) o ('NO_APLICA', []) o None si fallo."""
    key = tt._secret("OPENROUTER_API_KEY") or ""
    if not key or not HERMES_BIN.exists() or not VENV_PY.exists():
        return None
    work = OUT_DIR / f"{aid[:8]}-{_slug(d.get('titulo', ''))}"
    (work / "adjuntos").mkdir(parents=True, exist_ok=True)
    os.chmod(work, 0o700)
    # las imagenes se bajan aqui (token solo a graph.microsoft.com); el agente las ve con su herramienta de vision
    leidos, sin_leer, tok = [], [], None
    for r in d.get("recursos_profe") or []:
        nombre, link = r.get("nombre") or "adjunto", r.get("link") or ""
        if nombre.lower().rsplit(".", 1)[-1] not in ("jpg", "jpeg", "png", "webp") or not link or len(leidos) >= MAX_IMG:
            sin_leer.append(nombre)
            continue
        try:
            tok = tok or tt.get_access_token()
            (work / "adjuntos" / _slug(nombre.rsplit(".", 1)[0])).with_suffix("." + nombre.rsplit(".", 1)[-1].lower()).write_bytes(
                _bajar_graph(link + "/content", tok))
            leidos.append(nombre)
        except Exception as e:  # noqa: BLE001
            print(f"[borrador] no pude bajar {nombre}: {str(e)[:80]}", file=sys.stderr)
            sin_leer.append(nombre)
    nombres = sorted(x.name for x in (work / "adjuntos").iterdir())
    import shutil
    import tempfile
    perfil = HERMES / "profiles" / PERFIL
    copia = Path(tempfile.mkdtemp(prefix="tareas-perfil-"))
    try:
        for f in ("SOUL.md", "config.yaml", "profile.yaml", "skills", "memories"):
            if (perfil / f).is_dir():
                shutil.copytree(perfil / f, copia / f, symlinks=False)
            elif (perfil / f).is_file():
                shutil.copy2(perfil / f, copia / f)
        caja = _sandbox(work, copia)
        if caja is None:
            print("[borrador] sin bwrap: el agente no corre sin aislamiento", file=sys.stderr)
            return None
        r = _correr_agente(caja, d, nombres, sin_leer, key, work)
    finally:
        shutil.rmtree(copia, ignore_errors=True)
    return r


def _correr_agente(caja, d, nombres, sin_leer, key, work):
    try:
        r = subprocess.run(caja + [str(HERMES_BIN), "-p", PERFIL, "chat", "-q", _prompt_agente(d, nombres, sin_leer), "-Q",
                                   "--source", "tool", "--max-turns", "40"],
                           env={"PATH": "/usr/local/bin:/usr/bin:/bin", "HOME": str(Path.home()), "LANG": "C.UTF-8", "OPENROUTER_API_KEY": key},
                           cwd=str(work), capture_output=True, text=True, timeout=900)
    except (subprocess.TimeoutExpired, OSError) as e:
        print(f"[borrador] agente: {type(e).__name__}", file=sys.stderr)
        return None
    salida = r.stdout or ""
    if "NO_APLICA" in salida.split("ARCHIVO:")[0][-400:] and "ARCHIVO:" not in salida:
        return "NO_APLICA", []
    raiz = work.resolve()
    archivos = [f for f in sorted(work.rglob("*")) if f.suffix.lower() in SALIDAS and "adjuntos" not in f.parts
                and not f.is_symlink() and f.is_file() and f.resolve().is_relative_to(raiz)
                and f.stat().st_nlink == 1 and f.stat().st_size < 25_000_000][:5]
    if r.returncode != 0 or not archivos:
        print(f"[borrador] agente sin archivos (rc={r.returncode}): {(r.stderr or salida)[-200:]}", file=sys.stderr)
        return None
    for f in archivos:
        os.chmod(f, 0o600)
    return work, archivos


def _avisa(msg: str) -> None:
    subprocess.run([str(NOTIFY), msg], env=dict(os.environ, MOCHI_ROUTE="0"), capture_output=True, text=True, timeout=120)


def crear(aid: str, force: bool = False, notificar: bool = True) -> str:
    """Devuelve 'ok:<ruta>', 'ya', 'repetida:<ruta>', 'no_aplica' o 'error:<motivo>'."""
    tt = _load(HERMES / "plugins" / "teams-tareas" / "__init__.py", "teams_tareas")
    try:
        d = json.loads(tt.tarea_detalle({"ref": aid}))
    except Exception as e:  # noqa: BLE001
        return f"error:detalle {e}"
    if not d or d.get("error"):
        return f"error:{(d or {}).get('error', 'sin detalle')}"
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    existente = next(OUT_DIR.glob(f"{aid[:8]}-*.md"), None)
    if existente and not force:
        return "ya"
    instr = (d.get("instrucciones") or "").strip()
    if not instr and not d.get("recursos_profe"):
        return "no_aplica"
    if "youtube.com" in instr.lower() or "youtu.be" in instr.lower() or "webinar" in (d.get("titulo") or "").lower():
        return "no_aplica"   # webinar: depende de haberlo visto; la asistencia no se automatiza
    huella = _huella(d)
    idx = _index()
    previo = idx.get(huella)
    if previo and Path(previo["ruta"]).exists() and not force:
        previos = [Path(x) for x in previo.get("archivos") or [] if Path(x).exists()] or \
                  [Path(previo["ruta"]).with_suffix(".docx")]
        previos = [x for x in previos if x.exists()]
        estado = ", ".join(_adjuntar(tt, aid, x) for x in previos) if previos else "sin archivo"
        if notificar:
            _avisa(f"📝 «{d.get('titulo')}» ({d.get('clase')}) pide lo mismo que «{previo['titulo']}» ({previo['clase']}): "
                   f"no lo hice dos veces, reutilicé el mismo trabajo ({estado} en esta tarea). Está en el server: {previo['ruta']}")
        return f"repetida:{previo['ruta']}"
    hecho = _agente(tt, d, aid)
    if hecho and hecho[0] == "NO_APLICA":
        return "no_aplica"
    if hecho:
        work, archivos = hecho
        ruta = work.with_suffix(".md")
        ruta.write_text(f"# {d.get('titulo')} ({d.get('clase')})\nHecho por el agente `{PERFIL}`. Archivos:\n" +
                        "\n".join(f"- {a}" for a in archivos) + "\n", encoding="utf-8")
        os.chmod(ruta, 0o600)
        estado = ", ".join(f"{a.name}: {_adjuntar(tt, aid, a)}" for a in archivos)
        idx[huella] = {"ruta": str(ruta), "titulo": d.get("titulo"), "clase": d.get("clase"), "archivos": [str(a) for a in archivos]}
        INDEX.write_text(json.dumps(idx, ensure_ascii=False, indent=1))
        os.chmod(INDEX, 0o600)
        if notificar:
            _avisa(f"📝 Ya hice lo que pide «{d.get('titulo')}» ({d.get('clase')}) con el agente de tareas y lo dejé en Teams "
                   f"({estado}). Revísalo antes de entregar; lo que dice [ALAN] lo pones tú. En el server: {work}")
        return f"ok:{ruta} ({estado})"
    texto = generar(tt, d)
    if not texto:
        return "error:el modelo no devolvió texto"
    if texto.strip().upper().startswith("NO_APLICA"):
        return "no_aplica"
    texto = re.sub(r"-{4,}", "---", re.sub(r"[ \t]{2,}", " ", texto)).strip()   # el modelo a veces rellena las tablas
    texto = re.sub(r"(?m)^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)*\|?\s*$\n?", "", texto)   # fila de alineacion de tablas
    if len(texto) > 20000:
        return "error:salida degenerada (demasiado larga)"
    primera, _, resto = texto.partition("\n")
    if primera.lower().startswith(("aquí tienes", "aqui tienes", "claro", "por supuesto")) and resto.strip():
        texto = resto.strip()   # la frase de cortesía del modelo no es parte del trabajo
    ruta = OUT_DIR / f"{aid[:8]}-{_slug(d.get('titulo', ''))}.md"
    ruta.write_text(
        f"# Borrador — {d.get('titulo')} ({d.get('clase')})\n"
        f"_Vence {d.get('vence')} · generado {datetime.now().isoformat(timespec='minutes')} · "
        f"NO entregado: revísalo y complétalo antes de usarlo._\n\n{texto}\n",
        encoding="utf-8",
    )
    os.chmod(ruta, 0o600)
    docx = ruta.with_suffix(".docx")
    _md_a_docx(texto, f"{d.get('titulo')} — {d.get('clase')}", docx)
    os.chmod(docx, 0o600)
    estado = _adjuntar(tt, aid, docx)
    idx[huella] = {"ruta": str(ruta), "titulo": d.get("titulo"), "clase": d.get("clase")}
    INDEX.write_text(json.dumps(idx, ensure_ascii=False, indent=1))
    os.chmod(INDEX, 0o600)
    if notificar:
        cuerpo = texto if len(texto) <= TELEGRAM_CAP else texto[:TELEGRAM_CAP] + "\n[…recortado; completo en el server]"
        _avisa(f"📝 Ya hice lo que pide «{d.get('titulo')}» ({d.get('clase')}) y lo dejé en Teams: {estado}. "
               f"Revísalo antes de entregar; lo que dice [ALAN] o [BUSCAR] lo pones tú. Copia en el server: {ruta}\n\n{cuerpo}")
    return f"ok:{ruta} ({estado})"


def main(argv: list[str]) -> int:
    args = [a for a in argv[1:] if not a.startswith("--")]
    if not args:
        print(__doc__)
        return 2
    res = crear(args[0], force="--force" in argv, notificar="--no-notify" not in argv)
    print(res)
    return 0 if res.startswith(("ok", "ya", "repetida", "no_aplica")) else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
