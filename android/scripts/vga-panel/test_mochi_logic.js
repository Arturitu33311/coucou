"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const L = require("./mochi_logic.js");

test("the VGA panel is the failsafe: it owns the screen only when it is the active device", () => {
  assert.equal(L.isMine("vga"), true);
  assert.equal(L.isMine("pc"), false);
  assert.equal(L.isMine("s21"), false);
  assert.equal(L.isMine("s22"), false);          // the old rule (`!== "pc"`) got this wrong
  assert.equal(L.isMine(undefined), true);       // no word from the server: nobody else is known
});

test("device labels are plain Spanish", () => {
  assert.equal(L.deviceLabel("pc"), "la PC");
  assert.equal(L.deviceLabel("s22"), "el S22");
  assert.equal(L.deviceLabel("vga"), "este panel");
  assert.equal(L.deviceLabel("otro"), "otro dispositivo");
});

test("emotion decides the calm mood with the panel's original thresholds", () => {
  assert.equal(L.emotionMood({ affection: 90, patience: 80, trust: 85 }), "Happy");
  assert.equal(L.emotionMood({ affection: 30, patience: 40, trust: 20 }), "Sleepy");
  assert.equal(L.emotionMood({ affection: 60, patience: 60, trust: 60 }), "Idle");
  assert.equal(L.emotionMood({}), "Idle");
  assert.equal(L.emotionMood(null), "Idle");
});

test("seasonal outfits follow the Mac's rules", () => {
  assert.equal(L.seasonal(new Date(2026, 9, 13)), "witchHat");      // October
  assert.equal(L.seasonal(new Date(2026, 10, 1)), "witchHat");      // Nov 1
  assert.equal(L.seasonal(new Date(2026, 11, 10)), "santaHat");
  assert.equal(L.seasonal(new Date(2026, 11, 31)), "partyHat");
  assert.equal(L.seasonal(new Date(2027, 0, 2)), "partyHat");
  assert.equal(L.seasonal(new Date(2026, 6, 15)), "sunglasses");    // July
  assert.equal(L.seasonal(new Date(2026, 2, 3)), null);
  assert.equal(L.seasonal(new Date(2026, 3, 5)), "bunnyEars");      // Easter 2026 is April 5
});

test("auto outfit: night cap asleep, season otherwise, headphones while dancing", () => {
  const march = new Date(2026, 2, 3);
  assert.equal(L.pickOutfit("auto", "Sleepy", new Date(2026, 9, 13)), "nightcap");
  assert.equal(L.pickOutfit("auto", "Idle", new Date(2026, 9, 13)), "witchHat");
  assert.equal(L.pickOutfit("auto", "Dancing", march), "headphones");
  assert.equal(L.pickOutfit("auto", "Idle", march), null);
  assert.equal(L.pickOutfit("none", "Idle", new Date(2026, 9, 13)), null);
  assert.equal(L.pickOutfit("crown", "Idle", march), "crown");
  assert.equal(L.pickOutfit("???", "Idle", new Date(2026, 9, 13)), "witchHat");   // unknown reads as auto
});

test("mood decision: a forced mood wins, then ownership, then messages, then the calm mood", () => {
  const base = { emotion: { affection: 60, patience: 60, trust: 60 }, hour: 15, mine: true, unread: [], forced: null, now: 1000 };
  assert.equal(L.decideMood(base), "Idle");
  assert.equal(L.decideMood({ ...base, hour: 3 }), "Sleepy");                       // night
  assert.equal(L.decideMood({ ...base, unread: [{ kind: "alert" }] }), "Alert");
  assert.equal(L.decideMood({ ...base, unread: [{ kind: "warn" }] }), "Surprised");
  assert.equal(L.decideMood({ ...base, unread: [{ kind: "info", mood: "Celebrating" }] }), "Celebrating");
  assert.equal(L.decideMood({ ...base, unread: [{ kind: "info" }] }), "Happy");
  // not mine: another device has Alan's attention, so this panel rests (it only wakes as the failsafe)
  assert.equal(L.decideMood({ ...base, mine: false, unread: [{ kind: "alert" }] }), "Sleepy");
  assert.equal(L.decideMood({ ...base, mine: false, emotion: { affection: 90, patience: 80, trust: 85 } }), "Sleepy");
  // forced (e.g. happy for a few seconds after an ack) beats everything while it lasts
  assert.equal(L.decideMood({ ...base, forced: { mood: "Happy", until: 2000 }, unread: [{ kind: "alert" }] }), "Happy");
  assert.equal(L.decideMood({ ...base, forced: { mood: "Happy", until: 500 } }), "Idle");   // expired
});

test("gestures: never none, never the same twice in a row, mornings lean to yawns", () => {
  let seed = 7;
  const rnd = () => ((seed = (seed * 1664525 + 1013904223) % 4294967296) / 4294967296);
  let prev = "None";
  const seen = new Set();
  for (let i = 0; i < 300; i++) {
    const g = L.nextGesture(prev, 15, rnd);
    assert.notEqual(g, "None");
    assert.notEqual(g, prev);
    seen.add(g);
    prev = g;
  }
  assert.ok(seen.size >= 7);
  let yawnsMorning = 0, yawnsNoon = 0;
  for (let i = 0; i < 2000; i++) {
    if (L.nextGesture("Hop", 8, rnd) === "Yawn") yawnsMorning++;
    if (L.nextGesture("Hop", 13, rnd) === "Yawn") yawnsNoon++;
  }
  assert.ok(yawnsMorning > yawnsNoon * 1.8);
});

test("gesture timing is calm", () => {
  for (const g of ["Yawn", "Stretch", "LookUp", "LookAround", "Hop", "Whistle", "Scratch", "Wink", "Think"]) {
    assert.ok(L.gestureMs(g) >= 700 && L.gestureMs(g) <= 3600, g);
  }
  assert.equal(L.gestureMs("None"), 0);
  for (const r of [0, 0.5, 0.999]) {
    const gap = L.gapMs(() => r);
    assert.ok(gap >= 6000 && gap <= 15000);
  }
});

test("outfit fit shrinks him so a witch hat stays inside the canvas", () => {
  assert.equal(L.fitFor(null), 1);
  assert.ok(L.fitFor("witchHat") < L.fitFor("crown"));
  assert.ok(L.fitFor("witchHat") <= 0.7);
});

test("the big stage gives outfits more room than the island does, but the tall party hat stays inside", () => {
  assert.ok(L.stageFit("witchHat") > L.fitFor("witchHat"));
  assert.ok(L.stageFit("witchHat") <= 1);
  assert.ok(L.stageFit("partyHat") <= 0.8);          // its tip was cut at 0.89
  assert.equal(L.stageFit(null), 1);
  for (const o of ["witchHat", "santaHat", "crown", "bunnyEars", "headphones", "scarf", "pumpkin", "nightcap"]) {
    assert.ok(L.stageFit(o) <= 1 && L.stageFit(o) >= L.fitFor(o), o);
  }
});

test("what the API sends is never trusted as markup: numbers are coerced, classes are whitelisted", () => {
  assert.equal(L.num(20), "20");
  assert.equal(L.num("21.5"), "21.5");
  assert.equal(L.num("<img src=x onerror=alert(1)>"), "–");
  assert.equal(L.num(null), "–");
  assert.equal(L.num(undefined), "–");
  assert.equal(L.num(NaN), "–");
  assert.equal(L.num(Infinity), "–");
  assert.equal(L.trendKey("up"), "up");
  assert.equal(L.trendKey("down"), "down");
  assert.equal(L.trendKey("flat"), "flat");
  assert.equal(L.trendKey('x" onmouseover="alert(1)'), "flat");
  assert.equal(L.trendKey(undefined), "flat");
});

test("night is between midnight and six", () => {
  assert.equal(L.isNight(0), true);
  assert.equal(L.isNight(5), true);
  assert.equal(L.isNight(6), false);
  assert.equal(L.isNight(23), false);
});
