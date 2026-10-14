/* Mochi del panel VGA: dibujo en canvas 2D, en GRANDE.
 * Es el Mochi actual de Coucou (MochiGlyph en Mochi.kt: mismos estados de ánimo, mismos gestos, mismos
 * ojos) con el detalle que da el tamaño de este panel: cuerpo con gradiente y luz, brillos en los ojos,
 * sombra al suelo y glow de fósforo. El vestuario es el MISMO código que dibuja el de la PC
 * (window.MochiWardrobe, empaquetado desde wardrobe.ts).
 *
 * Todo se dibuja en una caja de 360×360 unidades; el canvas la escala a su tamaño real. */
(function (root) {
  "use strict";
  const BOX = 360;
  const CENTER_Y = 206;                 // centro del cuerpo en la caja
  const GROUND_Y = 322;                 // suelo: la sombra queda justo bajo el cuerpo
  const INK = "#04291c";
  const WORKING = "#4F9BFF", SEARCHING = "#6365F2", THINKING = "#8B5CF6";
  const CONFETTI = ["#E08A6C", "#FFFFFF", "#F5A524", "#7DD3FC", "#4ADE80", "#C4B5FD"];
  const TAU = Math.PI * 2;

  function squircle(x, x0, y0, w, h, r) {
    x.beginPath();
    x.moveTo(x0 + r, y0);
    x.arcTo(x0 + w, y0, x0 + w, y0 + h, r);
    x.arcTo(x0 + w, y0 + h, x0, y0 + h, r);
    x.arcTo(x0, y0 + h, x0, y0, r);
    x.arcTo(x0, y0, x0 + w, y0, r);
    x.closePath();
  }

  const FONT = "'JetBrains Mono', ui-monospace, monospace";

  function text(x, ch, cx, cy, size, color, alpha, bold) {
    x.save();
    x.globalAlpha = Math.max(0, Math.min(1, alpha));
    x.fillStyle = color;
    x.font = (bold === false ? "400 " : "700 ") + Math.round(size) + "px " + FONT;
    x.textAlign = "center";
    x.textBaseline = "middle";
    x.fillText(ch, cx, cy);
    x.restore();
  }

  /**
   * canvas: el elemento. wardrobe: window.MochiWardrobe (puede faltar: sin trajes).
   * s (estado): { mood, outfit, gesture, gp (0..1), thought, lookX, t (s) }
   */
  function create(canvas, wardrobe) {
    const ctx = canvas.getContext("2d");
    const scale = canvas.width / BOX;

    function draw(s) {
      const x = ctx;
      const t = s.t || 0;
      const mood = s.mood || "Idle";
      const g = mood === "Idle" ? (s.gesture || "None") : "None";
      const gp = Math.max(0, Math.min(1, s.gp || 0));
      const env = g === "None" ? 0 : Math.sin(Math.PI * gp);

      // Relojes del cuerpo (los de MochiGlyph): parpadeo 3.8 s, vaivén 1.1 s, compás 1.4 s, vistazo 12 s.
      const blinkPhase = (t % 3.8) * 1000;
      const blink = blinkPhase < 3600 ? 1 : blinkPhase < 3700 ? 1 - 0.92 * ((blinkPhase - 3600) / 100)
        : 0.08 + 0.92 * ((blinkPhase - 3700) / 100);
      const bobP = (t % 1.1) / 1.1;
      const bob = bobP < 0.5 ? bobP * 2 : (1 - bobP) * 2;
      const beat = (t % 1.4) / 1.4;
      const wave = beat * TAU;
      const pk = (t % 12) * 1000;
      const peek = pk < 8000 ? 0 : pk < 9000 ? (pk - 8000) / 1000 : pk < 10100 ? 1 - (pk - 9000) / 1100
        : pk < 11100 ? -(pk - 10100) / 1000 : -1 + (pk - 11100) / 900;

      const dancing = mood === "Dancing", surprised = mood === "Surprised", tired = mood === "Tired";
      const celebrating = mood === "Celebrating", glancing = mood === "Glance";
      const listening = mood === "Listening", sleepy = mood === "Sleepy", alert = mood === "Alert";
      const happyMood = mood === "Happy";
      const lookX = s.lookX || 0;

      // sorpresa: un resorte de una vez
      const popT = Math.min(1, Math.max(0, s.popT == null ? 1 : s.popT));
      const pop = surprised ? 1 - Math.exp(-6 * popT) * Math.cos(10 * popT) : 0;

      const tilt = (dancing ? Math.sin(wave) * 13 : celebrating ? Math.sin(wave) * 8 : tired ? 7
        : glancing ? lookX * 6
          : g === "Scratch" ? Math.sin(gp * 6 * Math.PI) * 6 * env
            : g === "Whistle" ? Math.sin(gp * 2 * Math.PI) * 4
              : g === "Tickle" ? Math.sin(gp * 10 * Math.PI) * 10 * env
                : g === "Hop" ? Math.sin(gp * 2 * Math.PI) * 3 : 0) * Math.PI / 180;
      const shiftX = dancing ? Math.sin(wave) * 0.07 : surprised ? (bob - 0.5) * 0.05 : 0;
      const hop = dancing ? -Math.abs(Math.sin(wave)) * 0.09
        : celebrating ? -Math.abs(Math.sin(wave)) * 0.13
          : g === "Hop" ? -Math.abs(Math.sin(gp * 2 * Math.PI)) * 0.16
            : g === "Tickle" ? -Math.abs(Math.sin(gp * 5 * Math.PI)) * 0.05 * env : 0;
      const breathe = !dancing && !surprised && !tired ? 1 + (bob - 0.5) * 0.03 : 1;
      const popScale = surprised ? 0.6 + pop * 0.4 : 1;
      const stretchY = g === "Stretch" ? 1 + 0.14 * env : g === "Yawn" ? 1 + 0.05 * env : 1;
      const stretchX = g === "Stretch" ? 1 - 0.05 * env : 1;
      const fit = s.fit || 1;

      x.setTransform(scale, 0, 0, scale, 0, 0);
      x.clearRect(0, 0, BOX, BOX);

      // ── geometría del cuerpo (la de MochiGlyph, sobre una caja de 360) ──
      const W = 270;                                  // ancho del cuerpo
      const bodyH = W * 0.80 * (surprised ? 1.06 : 1);
      const groundY = GROUND_Y;
      const cx0 = BOX / 2;
      const lift = alert ? -bob * BOX * 0.07 : happyMood ? -bob * BOX * 0.035
        : surprised ? -pop * BOX * 0.08 : dancing ? -Math.abs(Math.sin(wave)) * BOX * 0.05 : 0;

      // sombra al suelo (no se mueve con el cuerpo, solo se afina cuando salta)
      const air = Math.min(1, Math.max(0, (-(hop * BOX) - lift) / 60));
      for (const [ry, a] of [[17, 0.16], [13, 0.26], [9, 0.38]]) {
        x.fillStyle = "rgba(0,12,8," + (a * (1 - 0.4 * air)) + ")";
        x.beginPath();
        x.ellipse(cx0, groundY, (W * 0.44) * fit * (1 - 0.25 * air), ry * fit, 0, 0, TAU);
        x.fill();
      }

      x.save();
      // todo el Mochi (cuerpo y traje) pivota sobre el suelo: así el traje chico no flota
      x.translate(cx0 + shiftX * W, groundY + hop * BOX);
      x.scale(fit * breathe * popScale * stretchX, fit * popScale * (dancing ? 2 - breathe : 1) * stretchY);
      x.translate(0, -(groundY - CENTER_Y));            // el centro del cuerpo queda en el origen
      x.rotate(tilt);                                   // …y gira sobre su centro, no sobre el suelo
      const top = -bodyH / 2 + lift;                      // cuerpo centrado en (0, 0)
      const headCy = top + bodyH / 2;

      // ── vestuario por detrás ──
      const R = W / 2.28;
      const yaw = glancing ? lookX * 0.45
        : mood === "Idle" ? peek * 0.35 + (g === "LookAround" ? Math.sin(gp * 2 * Math.PI) * 0.4 : 0)
          : dancing ? Math.sin(wave) * 0.25 : 0;
      let ward = null;
      if (wardrobe && s.outfit) {
        ward = {
          outfit: s.outfit,
          H: wardrobe.mochiH(R, yaw, 0, dancing ? Math.sin(wave) * 0.4 : celebrating ? Math.sin(wave) * 0.3 : 0,
            alert || happyMood ? -bob * 0.15 : 0),
          cx: 0, cy: headCy, tilt: 0, sx: 1, sy: 1, morph: 0, presence: s.presence == null ? 1 : s.presence,
          rollTurns: 0, t,
        };
        wardrobe.drawOutfitBehind(x, ward);
      }

      // ── cuerpo: gradiente vertical, luz arriba-izquierda, sombra interna, borde y glow ──
      const radius = bodyH * 0.42;
      const bx = -W / 2;
      const grad = x.createLinearGradient(0, top, 0, top + bodyH);
      // la calabaza recolorea todo el cuerpo (PumpkinTop/Bottom de la app)
      const tint = s.tint || (s.outfit === "pumpkin" ? ["#FFA94D", "#E8590C"] : null);
      if (tint) {
        grad.addColorStop(0, tint[0]); grad.addColorStop(1, tint[1]);
      } else {
        grad.addColorStop(0, "#3fe0a0"); grad.addColorStop(0.45, "#10b981"); grad.addColorStop(1, "#087a53");
      }
      x.save();
      x.shadowColor = "rgba(16,185,129,.55)";
      x.shadowBlur = 30;
      squircle(x, bx, top, W, bodyH, radius);
      x.fillStyle = grad;
      x.fill();
      x.restore();
      squircle(x, bx, top, W, bodyH, radius);
      x.lineWidth = 5;
      x.strokeStyle = "#05402b";
      x.stroke();
      const hl = x.createRadialGradient(bx + W * 0.3, top + bodyH * 0.25, 8, bx + W * 0.3, top + bodyH * 0.25, W * 0.6);
      hl.addColorStop(0, "rgba(255,255,255,.36)");
      hl.addColorStop(1, "rgba(255,255,255,0)");
      squircle(x, bx, top, W, bodyH, radius);
      x.fillStyle = hl;
      x.fill();
      const sh = x.createLinearGradient(0, top + bodyH * 0.66, 0, top + bodyH);
      sh.addColorStop(0, "rgba(0,0,0,0)");
      sh.addColorStop(1, "rgba(0,40,25,.40)");
      squircle(x, bx, top, W, bodyH, radius);
      x.fillStyle = sh;
      x.fill();

      // ── ojos (los de MochiGlyph) ──
      // With a cap on, the brim comes down over a forehead-high pair of eyes: they sit where the wardrobe's own
      // head model puts them (a little below the centre), so the cap frames them instead of hiding them.
      const CAPS = { witchHat: 1, santaHat: 1, partyHat: 1, crown: 1, beanie: 1, nightcap: 1, bunnyEars: 1 };
      const eyeBase = s.outfit && CAPS[s.outfit] ? headCy + bodyH * 0.09 : top + bodyH * 0.46;
      const eyeY = eyeBase - (g === "LookUp" ? bodyH * 0.07 * env : 0);
      const lean = listening ? W * 0.04 : 0;
      const glanceShift = glancing ? lookX * W * 0.10 : 0;
      const idleShift = mood === "Idle" ? peek * W * 0.07 + (g === "LookAround" ? Math.sin(gp * 2 * Math.PI) * W * 0.08 : 0) : 0;
      const contented = (s.charging && mood === "Idle" && g === "None") || g === "Tickle";
      const happyEyes = happyMood || dancing || celebrating || contented;
      const eyesShut = sleepy || (g === "Yawn" && env > 0.25) || (g === "Stretch" && env > 0.25);
      const xs = [bx + W * 0.32 + lean + glanceShift + idleShift, bx + W * 0.68 + lean + glanceShift + idleShift];
      x.fillStyle = INK;
      x.strokeStyle = INK;
      x.lineCap = "round";
      xs.forEach((cx, idx) => {
        const shine = (rx, ry) => {
          x.fillStyle = "rgba(255,255,255,.92)";
          x.beginPath(); x.arc(cx + rx * 0.32, eyeY - ry * 0.42, Math.max(2, rx * 0.34), 0, TAU); x.fill();
          x.beginPath(); x.arc(cx - rx * 0.28, eyeY + ry * 0.42, Math.max(1.5, rx * 0.15), 0, TAU); x.fill();
          x.fillStyle = INK;
        };
        const oval = (rx, ry) => { x.beginPath(); x.ellipse(cx, eyeY, rx, Math.max(0.6, ry), 0, 0, TAU); x.fill(); };
        if (eyesShut) {
          const r = W * 0.07;
          x.lineWidth = W * 0.05;
          x.beginPath(); x.moveTo(cx - r, eyeY + r * 0.2); x.lineTo(cx + r, eyeY + r * 0.2); x.stroke();
        } else if (happyEyes || (g === "Wink" && idx === 1 && env > 0.3)) {
          const r = W * 0.085;
          x.lineWidth = W * 0.055;
          x.beginPath(); x.moveTo(cx - r, eyeY + r * 0.6); x.quadraticCurveTo(cx, eyeY - r * 1.4, cx + r, eyeY + r * 0.6); x.stroke();
        } else if (surprised) {
          const rx = W * 0.08, ry = rx * 2.05;
          oval(rx, ry); shine(rx, ry);
        } else if (tired) {
          const rx = W * 0.062, ry = Math.max(0.5, rx * 0.55) * (0.35 + 0.65 * blink);
          oval(rx, ry);
        } else if (mood === "Error") {
          const r = W * 0.075;
          x.lineWidth = W * 0.05;
          x.beginPath(); x.moveTo(cx - r, eyeY); x.lineTo(cx + r, eyeY); x.stroke();
        } else if (glancing) {
          const rx = W * 0.062, ry = rx * 1.6 * blink;
          oval(rx, ry); if (blink > 0.6) shine(rx, ry);
        } else {
          const rx = W * (alert ? 0.075 : 0.058), ry = rx * 1.55 * blink;
          oval(rx, ry); if (blink > 0.6) shine(rx, ry);
        }
      });

      if (happyEyes && !eyesShut) {
        x.fillStyle = "rgba(255,170,150,.30)";
        for (const cx of xs) {
          x.beginPath(); x.ellipse(cx + (cx < bx + W / 2 ? -W * 0.02 : W * 0.02), eyeY + bodyH * 0.16, W * 0.055, W * 0.032, 0, 0, TAU); x.fill();
        }
      }
      // ── vestuario por delante (gafas sobre los ojos, etc.) ──
      if (ward) wardrobe.drawOutfitFront(x, ward);

      // ── bocas y decoraciones propias de cada estado ──
      if (g === "Yawn" && env > 0.1) {
        x.fillStyle = INK;
        x.beginPath(); x.ellipse(bx + W / 2, eyeY + bodyH * 0.24 + W * 0.10 * env, W * 0.07 * env, W * 0.10 * env, 0, 0, TAU); x.fill();
      }
      if (g === "Whistle") {
        x.fillStyle = INK;
        x.beginPath(); x.arc(bx + W / 2, eyeY + bodyH * 0.27, W * 0.035, 0, TAU); x.fill();
        text(x, "♪", bx + W * 0.80, top - BOX * (0.02 + gp * 0.22), W * 0.30, "#e9fff0", 1 - gp);
      }
      if (g === "Think") {
        const a = Math.min(1, env * 3);
        x.fillStyle = "rgba(255,255,255," + (0.92 * a) + ")";
        for (const [px, py, pr] of [[0.70, 0.02, 0.035], [0.80, -0.07, 0.055], [0.78, -0.24, 0.24]]) {
          x.beginPath();
          x.arc(bx + W * px, top + (py < -0.2 ? -BOX * 0.20 : py < 0 ? -BOX * 0.05 : bodyH * 0.02) - (py === -0.24 ? 14 : 0),
            W * pr, 0, TAU);
          x.fill();
        }
        text(x, s.thought || "💭", bx + W * 0.78, top - BOX * 0.20 - 14, W * 0.30, "#04291c", a);
      }
      if (s.charging && mood === "Idle") {
        text(x, "⚡", bx + W * 0.92, top + BOX * 0.0, W * 0.28, "#FACC15", 0.6 + 0.4 * bob);
      }
      if (mood === "Error") {
        x.fillStyle = "#EF4444";
        x.beginPath(); x.arc(bx + W * 0.5, top - BOX * 0.03, W * 0.06, 0, TAU); x.fill();
      }
      if (mood === "Working" || mood === "Searching" || mood === "Thinking") {
        const phase = Math.floor(beat * 3) % 3;
        const col = mood === "Searching" ? SEARCHING : mood === "Thinking" ? THINKING : WORKING;
        for (let i = 0; i < 3; i++) {
          x.globalAlpha = i === phase ? 1 : 0.35;
          x.fillStyle = col;
          x.beginPath(); x.arc(bx + W * (0.30 + i * 0.20), top - BOX * 0.04, W * 0.045, 0, TAU); x.fill();
        }
        x.globalAlpha = 1;
      }
      if (surprised && pop > 0.4) text(x, "!", bx + W * 0.5, top - BOX * 0.075, W * 0.27, "#e9fff0", pop);
      if (dancing) {
        const up = (Math.sin(wave) + 1) / 2;
        text(x, "♪", bx + W * 0.10, top - BOX * (0.0 + up * 0.10), W * 0.24, "#e9fff0", 0.35 + up * 0.65);
        text(x, "♫", bx + W * 0.90, top - BOX * (0.0 + (1 - up) * 0.10), W * 0.22, "#e9fff0", 0.35 + (1 - up) * 0.65);
      }
      if (sleepy) {
        const shut = 1 - blink;
        text(x, "z", bx + W * 0.90, top - BOX * (0.04 + shut * 0.12 + 0.02 * Math.sin(t * 1.2)), W * (0.22 + shut * 0.10), "#95d3ba", 0.45 + shut * 0.5);
        text(x, "z", bx + W * 1.02, top - BOX * (0.12 + shut * 0.10), W * 0.15, "#95d3ba", 0.3 + shut * 0.4);
      }
      if (tired) {
        const dx = bx + W * 0.94, dy = top + bodyH * (0.18 + bob * 0.10), r = W * 0.045;
        x.fillStyle = "#7DD3FC";
        x.beginPath(); x.arc(dx, dy, r, 0, TAU); x.fill();
        x.beginPath();
        x.moveTo(dx - r * 0.8, dy - r * 0.2);
        x.quadraticCurveTo(dx, dy - r * 1.8, dx + r * 0.8, dy - r * 0.2);
        x.closePath(); x.fill();
      }
      if (celebrating) {
        const cy0 = top + bodyH * 0.4;
        for (let i = 0; i < 6; i++) {
          const a = wave + i * TAU / 6;
          const orbit = W * (0.50 + 0.08 * Math.sin(wave * 2 + i));
          x.fillStyle = CONFETTI[i % CONFETTI.length];
          x.beginPath(); x.arc(Math.cos(a) * orbit, cy0 + Math.sin(a) * orbit * 0.7, W * 0.032, 0, TAU); x.fill();
        }
      }
      x.restore();
    }

    return { draw, box: BOX };
  }

  root.MochiEngine = { create, BOX };
})(typeof self !== "undefined" ? self : this);
