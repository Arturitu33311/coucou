/* Mochi del panel VGA: lógica pura (sin DOM, sin canvas). Probada con `node --test`.
 * Las reglas son las del Mochi actual de Coucou (MochiIdleScript.kt, MochiWardrobe.kt, Arbiter.kt):
 * el panel es el RESPALDO de la casa: solo es "dueño" de los mensajes cuando la presencia lo señala
 * (active_device == "vga"); si Alan usa la PC o un teléfono, aquí Mochi se queda tranquilo. */
(function (root, factory) {
  if (typeof module === "object" && module.exports) module.exports = factory();
  else root.MochiLogic = factory();
})(typeof self !== "undefined" ? self : this, function () {
  "use strict";

  const LABELS = { pc: "la PC", s21: "el S21", s22: "el S22", vga: "este panel" };
  const deviceLabel = (d) => LABELS[d] || "otro dispositivo";

  /** El panel es el failsafe: suyo solo si la presencia dice "vga"; sin respuesta, nadie más es conocido. */
  const isMine = (activeDevice) => activeDevice === undefined || activeDevice === null || activeDevice === "vga";

  const isNight = (hour) => hour >= 0 && hour < 6;

  // Lo que manda la API nunca se interpola como marcado: los números se fuerzan y las clases van en lista blanca.
  const num = (v) => {
    if (v === null || v === undefined || v === "") return "–";
    const n = Number(v);
    return Number.isFinite(n) ? String(n) : "–";
  };
  const trendKey = (t) => (t === "up" || t === "down" || t === "flat" ? t : "flat");

  function emotionMood(e) {
    e = e || {};
    const vs = [e.affection, e.patience, e.trust].filter((v) => v != null);
    if (!vs.length) return "Idle";
    const avg = vs.reduce((a, b) => a + b, 0) / vs.length;
    if (avg >= 78) return "Happy";
    if (avg <= 40) return "Sleepy";
    return "Idle";
  }

  // ── Vestuario (MochiWardrobe.kt / outfits.ts, regla por regla) ───────────────
  function easter(year) {
    const a = year % 19, b = Math.floor(year / 100), c = year % 100;
    const d = Math.floor(b / 4), e = b % 4, f = Math.floor((b + 8) / 25);
    const g = Math.floor((b - f + 1) / 3);
    const h = (19 * a + b - d - g + 15) % 30;
    const i = Math.floor(c / 4), k = c % 4;
    const l = (32 + 2 * e + 2 * i - h - k) % 7;
    const m = Math.floor((a + 11 * h + 22 * l) / 451);
    return [Math.floor((h + l - 7 * m + 114) / 31), ((h + l - 7 * m + 114) % 31) + 1];
  }

  function seasonal(date) {
    const day = date.getDate(), month = date.getMonth() + 1, year = date.getFullYear();
    if ((month === 12 && day === 31) || (month === 1 && day <= 2)) return "partyHat";
    if (month === 12 && day <= 26) return "santaHat";
    if (month === 10 || (month === 11 && day === 1)) return "witchHat";
    const [em, ed] = easter(year);
    const days = Math.round((new Date(year, month - 1, day) - new Date(year, em - 1, ed)) / 86400000);
    if (days >= -2 && days <= 1) return "bunnyEars";
    if ((month === 6 && day >= 21) || month === 7 || month === 8) return "sunglasses";
    return null;
  }

  const OUTFITS = new Set(["partyHat", "beanie", "crown", "sunglasses", "roundGlasses", "bow", "scarf",
    "witchHat", "pumpkin", "santaHat", "bunnyEars", "headphones", "nightcap"]);

  /** setting: "auto" | "none" | un id de traje. Devuelve un id o null. */
  function pickOutfit(setting, mood, date) {
    if (setting === "none") return null;
    if (OUTFITS.has(setting)) return setting;
    if (mood === "Sleepy") return "nightcap";
    const s = seasonal(date);
    if (s) return s;
    if (mood === "Dancing") return "headphones";
    return null;
  }

  /** Cuánto se encoge todo el Mochi para que el traje quepa en su lienzo. */
  function fitFor(o) {
    switch (o) {
      case "witchHat": return 0.66;
      case "partyHat": return 0.74;
      case "bunnyEars": return 0.78;
      case "santaHat": case "nightcap": return 0.80;
      case "crown": case "beanie": return 0.84;
      case "headphones": case "pumpkin": return 0.88;
      case "bow": case "scarf": return 0.90;
      default: return 1;
    }
  }

  /** En el escenario grande los trajes tienen más espacio que en la isla (el Mochi está en un lienzo de 360);
   * el gorro de fiesta es alto y su punta se cortaba, así que no pasa de 0.8. */
  function stageFit(o) {
    const f = Math.min(1, fitFor(o) * 1.2);
    return o === "partyHat" ? Math.min(f, 0.8) : f;
  }

  // ── Estado de ánimo ───────────────────────────────────────────────────────────
  const HINTS = new Set(["Idle", "Happy", "Sleepy", "Alert", "Listening", "Celebrating", "Dancing", "Tired"]);
  const SEVERITY = { alert: 3, warn: 2, info: 1 };

  /** Lo que sienten los mensajes sin leer: alerta > aviso > info (con la pista de ánimo del mensaje). */
  function messageMood(unread) {
    if (!unread || !unread.length) return null;
    const top = unread.reduce((a, b) => ((SEVERITY[b.kind] || 1) > (SEVERITY[a.kind] || 1) ? b : a));
    if (top.kind === "alert") return "Alert";
    if (top.kind === "warn") return "Surprised";
    return HINTS.has(top.mood) ? top.mood : "Happy";
  }

  /** forced > no es mío (descansa) > mensajes sin leer > emoción/noche. */
  function decideMood(s) {
    if (s.forced && (!s.forced.until || s.forced.until > s.now)) return s.forced.mood;
    if (!s.mine) return "Sleepy";
    const calm = isNight(s.hour) ? "Sleepy" : emotionMood(s.emotion);
    return messageMood(s.unread) || calm;
  }

  // ── Gestos ociosos (MochiIdleScript.kt) ──────────────────────────────────────
  const REPERTOIRE = ["Yawn", "Stretch", "LookUp", "LookAround", "Hop", "Whistle", "Scratch", "Wink", "Think"];
  const DURATIONS = { None: 0, Yawn: 1800, Stretch: 1600, LookUp: 1400, LookAround: 2200, Hop: 900,
    Whistle: 2000, Scratch: 1200, Wink: 700, Think: 3500, Tickle: 1200 };

  const gestureMs = (g) => DURATIONS[g] || 0;
  const gapMs = (rnd) => 6000 + Math.floor(rnd() * 9001);

  function nextGesture(prev, hour, rnd) {
    const weighted = REPERTOIRE.filter((g) => g !== prev).map((g) => {
      let w = 1;
      if (hour >= 5 && hour <= 11 && (g === "Yawn" || g === "Stretch")) w = 4;
      else if (hour >= 14 && hour <= 17 && g === "Think") w = 3;
      return [g, w];
    });
    const total = weighted.reduce((a, [, w]) => a + w, 0);
    let pick = Math.floor(rnd() * total);
    for (const [g, w] of weighted) {
      if (pick < w) return g;
      pick -= w;
    }
    return weighted[weighted.length - 1][0];
  }

  const THOUGHTS = ["💭", "⭐", "🎵", "🍡"];
  function thoughtFor(hour, rnd) {
    const pool = THOUGHTS.slice();
    if (hour >= 5 && hour <= 11) pool.push("☀️");
    if (hour >= 14 && hour <= 17) pool.push("☕", "☕", "☕");
    if ((hour >= 0 && hour <= 4) || hour === 22 || hour === 23) pool.push("🌙", "🌙", "🌙");
    return pool[Math.floor(rnd() * pool.length)];
  }

  return { deviceLabel, isMine, isNight, num, trendKey, emotionMood, seasonal, pickOutfit, fitFor, stageFit, messageMood, decideMood,
    nextGesture, gestureMs, gapMs, thoughtFor, easter };
});
