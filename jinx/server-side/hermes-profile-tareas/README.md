# Perfil de Hermes `tareas` (agente de trabajos escolares, sin la personalidad de Jinx)

Vive en `~/.hermes/profiles/tareas/` del servidor. Aqui solo SOUL.md y config.yaml.
NO estan aqui: `memories/USER.md` (datos personales de Alan) ni `venv/` (python-docx 1.1.2 y python-pptx 1.0.2, creado con `python3 -m venv`).
Skills copiadas de ~/.hermes/skills: docx, pdf, xlsx, ocr-and-documents, grounded-citations, alan-academic-writing; instalada del hub: official/finance/pptx-author.
Lo invoca `scripts/borrador_tarea.py` con `hermes -p tareas chat -q ... -Q`, pasando OPENROUTER_API_KEY por entorno.

## Cómo habla Jinx con este agente
Vía oficial de Hermes: Kanban (`hermes kanban`, tarjetas asignadas a un perfil, despachador dentro del gateway). No se usa todavía: el trabajador lo lanza el gateway sin el sandbox bwrap y haría falta activar el toolset `kanban` y reiniciar el gateway. Hoy Jinx lo llama con la skill `tareas-agente` (`jinx-skill/SKILL.md`, copiada en `~/.hermes/skills/education/tareas-agente/`), que ejecuta `borrador_tarea.py` con su terminal.
