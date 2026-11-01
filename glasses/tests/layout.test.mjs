import test from "node:test";
import assert from "node:assert/strict";
import { COLUMNS, LINES_PER_PAGE, paginate, wrap, wrapLine, withFooter } from "../src/layout.ts";

test("wraps at word boundaries and never exceeds the width", () => {
  const lines = wrapLine("uno dos tres cuatro cinco seis siete ocho nueve diez once doce trece", 20);
  assert.ok(lines.every((l) => l.length <= 20), JSON.stringify(lines));
  assert.equal(lines.join(" "), "uno dos tres cuatro cinco seis siete ocho nueve diez once doce trece");
});

test("splits a word longer than the line", () => {
  const lines = wrapLine("x".repeat(50), 20);
  assert.deepEqual(lines.map((l) => l.length), [20, 20, 10]);
});

test("keeps one blank line between paragraphs and trims the ends", () => {
  assert.deepEqual(wrap("a\n\n\n\nb\n\n"), ["a", "", "b"]);
});

test("pages hold at most LINES_PER_PAGE lines and lose no text", () => {
  const text = Array.from({ length: 60 }, (_, i) => `palabra${i}`).join(" ");
  const pages = paginate(text);
  assert.ok(pages.length > 1);
  for (const p of pages) assert.ok(p.split("\n").length <= LINES_PER_PAGE);
  assert.equal(pages.join(" ").split(/\s+/).join(" "), text);
  for (const p of pages) for (const l of p.split("\n")) assert.ok(l.length <= COLUMNS);
});

test("empty text is one empty page; a page never starts with a blank line", () => {
  assert.deepEqual(paginate("   "), [""]);
  const text = "a ".repeat(30) + "\n\n" + "b ".repeat(30);
  for (const p of paginate(text, 3, 10)) assert.notEqual(p.split("\n")[0], "");
});

test("footer only when there is more than one page, with arrows at the right ends", () => {
  assert.equal(withFooter(["solo"], 0), "solo");
  assert.equal(withFooter(["a", "b", "c"], 0), "a\n  1/3 ›");
  assert.equal(withFooter(["a", "b", "c"], 1), "b\n‹ 2/3 ›");
  assert.equal(withFooter(["a", "b", "c"], 2), "c\n‹ 3/3");
  assert.equal(withFooter(["a", "b"], 9), "b\n‹ 2/2");
});
