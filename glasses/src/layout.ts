// What fits on the G2's 576×288 text container, as text we wrap ourselves so the firmware never has to.
// The font is proportional (measured in the simulator: ~9.3 px per character, so 54 columns ≈ 500 of 576 px, a
// safe margin for wide letters), and a page is 9 lines of
// text plus one footer line (10 × 27 px = 270 px of the 288 available; the firmware also caps a container at 999 bytes).

export const COLUMNS = 54;
export const LINES_PER_PAGE = 9;

/** Hard-wraps one paragraph at word boundaries; a word longer than the line is split. */
export function wrapLine(text: string, columns: number = COLUMNS): string[] {
  const out: string[] = [];
  let line = "";
  for (const raw of text.split(/\s+/)) {
    let word = raw;
    if (!word) continue;
    while (word.length > columns) {
      if (line) { out.push(line); line = ""; }
      out.push(word.slice(0, columns));
      word = word.slice(columns);
    }
    if (!line) line = word;
    else if (line.length + 1 + word.length <= columns) line += " " + word;
    else { out.push(line); line = word; }
  }
  if (line) out.push(line);
  return out;
}

/** The text as wrapped lines; blank lines between paragraphs are kept (one at most). */
export function wrap(text: string, columns: number = COLUMNS): string[] {
  const lines: string[] = [];
  for (const para of text.replace(/\r/g, "").split("\n")) {
    const w = wrapLine(para, columns);
    if (w.length === 0) {
      if (lines.length && lines[lines.length - 1] !== "") lines.push("");
    } else lines.push(...w);
  }
  while (lines.length && lines[lines.length - 1] === "") lines.pop();
  return lines;
}

/** Pages of up to `perPage` lines; an empty text is one empty page. */
export function paginate(text: string, perPage: number = LINES_PER_PAGE, columns: number = COLUMNS): string[] {
  const lines = wrap(text, columns);
  if (lines.length === 0) return [""];
  const pages: string[] = [];
  for (let i = 0; i < lines.length; i += perPage) {
    const chunk = lines.slice(i, i + perPage);
    while (chunk.length && chunk[0] === "" && chunk.length > 1) chunk.shift(); // no page starts with a blank line
    pages.push(chunk.join("\n"));
  }
  return pages;
}

/** A page with its position when there is more than one: "‹ 2/3 ›". */
export function withFooter(pages: string[], index: number): string {
  const page = pages[Math.max(0, Math.min(pages.length - 1, index))] ?? "";
  if (pages.length <= 1) return page;
  const at = Math.max(0, Math.min(pages.length - 1, index));
  return `${page}\n${at > 0 ? "‹ " : "  "}${at + 1}/${pages.length}${at < pages.length - 1 ? " ›" : ""}`;
}
