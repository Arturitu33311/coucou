# Perfil de Hermes `tareas` (agente de trabajos escolares, sin la personalidad de Jinx)

Vive en `~/.hermes/profiles/tareas/` del servidor. Aqui solo SOUL.md y config.yaml.
NO estan aqui: `memories/USER.md` (datos personales de Alan) ni `venv/` (python-docx 1.1.2 y python-pptx 1.0.2, creado con `python3 -m venv`).
Skills copiadas de ~/.hermes/skills: docx, pdf, xlsx, ocr-and-documents, grounded-citations, alan-academic-writing; instalada del hub: official/finance/pptx-author.
Lo invoca `scripts/borrador_tarea.py` con `hermes -p tareas chat -q ... -Q`, pasando OPENROUTER_API_KEY por entorno.
