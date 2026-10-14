#!/usr/bin/env python3
"""Arma dist/panel.html: panel.src.html con el vestuario, la lógica y el motor del Mochi dentro.

El server del panel (panel_server.py) sirve UN archivo, así que todo va inline. El vestuario se empaqueta antes
con esbuild (el mismo wardrobe.ts que dibuja el Mochi de la PC); `ESBUILD` puede apuntar a otro binario.
"""
import base64
import os
import subprocess
import sys

FONTS = [  # (familia, peso, archivo): las mismas del dashboard, incrustadas
    ("Familjen Grotesk", 500, "familjen-grotesk-500.woff2"),
    ("Familjen Grotesk", 600, "familjen-grotesk-600.woff2"),
    ("Familjen Grotesk", 700, "familjen-grotesk-700.woff2"),
    ("JetBrains Mono", 500, "jetbrains-mono-latin-500-normal.woff2"),
    ("JetBrains Mono", 700, "jetbrains-mono-latin-700-normal.woff2"),
]


def font_faces():
    out = []
    for family, weight, fname in FONTS:
        with open(os.path.join(HERE, "fonts", fname), "rb") as f:
            data = base64.b64encode(f.read()).decode("ascii")
        out.append("@font-face{font-family:'%s';font-weight:%d;font-style:normal;font-display:swap;"
                   "src:url(data:font/woff2;base64,%s) format('woff2');}" % (family, weight, data))
    return "\n".join(out)

HERE = os.path.dirname(os.path.abspath(__file__))
ESBUILD = os.environ.get(
    "ESBUILD", "/home/alan/dev/coucou-pc-improve/windows/node_modules/.bin/esbuild")


def read(name):
    with open(os.path.join(HERE, name), encoding="utf-8") as f:
        return f.read()


def main():
    dist = os.path.join(HERE, "dist")
    os.makedirs(dist, exist_ok=True)
    subprocess.run([ESBUILD, os.path.join(HERE, "bundle_entry.ts"), "--bundle", "--format=iife", "--minify",
                    "--target=es2019", "--outfile=" + os.path.join(dist, "mochi_wardrobe.js"), "--log-level=warning"],
                   check=True)
    page = read("panel.src.html")
    parts = {
        "/*@@WARDROBE@@*/": read("dist/mochi_wardrobe.js"),
        "/*@@LOGIC@@*/": read("mochi_logic.js"),
        "/*@@ENGINE@@*/": read("mochi_engine.js"),
        "/*@@CSS@@*/": read("panel.css"),
        "/*@@FONTS@@*/": font_faces(),
    }
    for marker, code in parts.items():
        if marker not in page:
            sys.exit("falta el marcador %s en panel.src.html" % marker)
        if "</script" in code.lower():
            sys.exit("el código a incrustar contiene </script")
        page = page.replace(marker, code)
    out = os.path.join(dist, "panel.html")
    with open(out, "w", encoding="utf-8") as f:
        f.write(page)
    print("ok: %s (%d bytes)" % (out, len(page.encode("utf-8"))))


if __name__ == "__main__":
    main()
