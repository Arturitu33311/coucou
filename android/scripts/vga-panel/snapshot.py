#!/usr/bin/env python3
"""Genera una página estática de PRUEBA del panel con `fetch` simulado (para capturas con Chromium headless).

  snapshot.py --status status.json --active vga --msg "Texto" --kind alert --out /ruta/panel_test.html
"""
import argparse
import json
import os
import re
import time

HERE = os.path.dirname(os.path.abspath(__file__))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--status", required=True)
    ap.add_argument("--page", default=os.path.join(HERE, "dist", "panel.html"))
    ap.add_argument("--active", default="vga")
    ap.add_argument("--msg", action="append", default=[])
    ap.add_argument("--kind", action="append", default=[])
    ap.add_argument("--hour", type=int, default=None, help="hora local simulada (0-23)")
    ap.add_argument("--date", default=None, help="fecha simulada YYYY-MM-DD (para el traje de temporada)")
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    with open(a.status, encoding="utf-8") as f:
        status = json.load(f)
    msgs = [{"id": i + 1, "text": t, "kind": (a.kind[i] if i < len(a.kind) else "info"),
             "source": "prueba", "ts": int(time.time()) - 120} for i, t in enumerate(a.msg)]
    stub = """<script>
(function(){
  const STATUS = %s, INBOX = %s, ACTIVE = %s;
  const real = window.fetch;
  window.fetch = function(url, opts){
    const u = String(url);
    const ok = body => Promise.resolve({ ok: true, json: () => Promise.resolve(body) });
    if(u.indexOf("/api/status") >= 0) return ok(STATUS);
    if(u.indexOf("/api/mochi/inbox") >= 0){
      if(ACTIVE === "none") return Promise.reject(new Error("sin presencia"));
      return ok({ messages: INBOX, active_device: ACTIVE, ts: Math.floor(Date.now()/1000) });
    }
    if(u.indexOf("/api/mochi/ack") >= 0) return ok({ ok: true });
    return real.apply(window, arguments);
  };
  %s
})();
</script>""" % (json.dumps(status), json.dumps(msgs), json.dumps(a.active), "")
    # hora/fecha simuladas: se sobreescribe Date para que traje y gestos sean reproducibles
    if a.hour is not None or a.date:
        y, mth, d = (int(x) for x in (a.date or time.strftime("%Y-%m-%d")).split("-"))
        h = a.hour if a.hour is not None else time.localtime().tm_hour
        stub += """<script>
(function(){
  const R = Date; const fixed = new R(%d, %d, %d, %d, 30, 0).getTime();
  const t0 = R.now();
  function F(...args){ return args.length ? new R(...args) : new R(fixed + (R.now() - t0)); }
  F.now = () => fixed + (R.now() - t0); F.UTC = R.UTC; F.parse = R.parse; F.prototype = R.prototype;
  window.Date = F;
})();
</script>""" % (y, mth - 1, d, h)
    with open(a.page, encoding="utf-8") as f:
        html = f.read()
    html = re.sub(r"<link[^>]*(fonts\.googleapis|fonts\.gstatic)[^>]*>", "", html)
    html = html.replace("<body>", "<body>" + stub, 1)
    with open(a.out, "w", encoding="utf-8") as f:
        f.write(html)
    print("ok:", a.out)


if __name__ == "__main__":
    main()
