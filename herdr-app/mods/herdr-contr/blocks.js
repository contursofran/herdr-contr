// Markdown split into the blocks the chat draws on its own: code fences and tables. One parser for both
// sides: mdblocks.js draws these blocks and estimate.js guesses their heights, so the two always see the
// same code boxes and tables. Pure (no gpui): the node tests import it as it is.

// A fence, as in CommonMark: ``` or ~~~ (3 or more), then an info string. The language is the info's first
// word ("```js title=a.js" is js). A backtick fence's info has no backtick.
const OPEN = /^\s*(`{3,}|~{3,})\s*([^`\s]*)[^`]*$/;
const TILDE_OPEN = /^\s*(~{3,})\s*(\S*).*$/;
// A closing fence: the same character, at least as many, nothing after it but spaces.
const closes = (line, fence) => {
  const m = /^\s*(`{3,}|~{3,})\s*$/.exec(line);
  return !!m && m[1][0] === fence[0] && m[1].length >= fence.length;
};

// A table's separator row: "| --- | :--: |".
export const TABLE_SEP = /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/;
// A table starts at a pipe line with a separator row under it.
export const tableStarts = (lines, i) => /^\s*\|/.test(lines[i]) && i + 1 < lines.length && TABLE_SEP.test(lines[i + 1]);

const alignOf = (cell) => {
  const c = cell.trim();
  return c.startsWith(":") && c.endsWith(":") ? "center" : c.endsWith(":") ? "right" : "left";
};

// "| a | b |" → ["a", "b"]. An escaped pipe and pipes inside `code` stay in the cell.
export function tableCells(line) {
  let s = line.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|") && !s.endsWith("\\|")) s = s.slice(0, -1);
  const out = []; let cur = "", code = false;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === "\\" && s[i + 1] === "|") { cur += "|"; i++; continue; }
    if (c === "`") code = !code;
    if (c === "|" && !code) { out.push(cur.trim()); cur = ""; continue; }
    cur += c;
  }
  out.push(cur.trim());
  return out;
}

// A line that is only one image link (#192): "![alt](src)", "![alt](<src with spaces>)", with an optional title.
export const IMAGE_LINE = /^ {0,3}!\[([^\]\n]*)\]\(\s*(<[^>\n]+>|[^\s()]+)(?:\s+(?:"[^"\n]*"|'[^'\n]*'))?\s*\)\s*$/;
const imageSrc = (m) => m[2].replace(/^<(.*)>$/, "$1");

// [{k:"md", text}, {k:"code", lang, text, lines}, {k:"table", head, align, rows}] in order. A code block's
// `lines` are its source lines (none for an empty box; `text` cannot tell that from one empty line). An
// unclosed fence runs to the end. A markdown block of blank lines only is left out.
// `img(src)`: the drawn image of a src (agentimages.js), or null. An image line outside code and tables whose src
// has one is a block of its own, {k:"img", alt, src, img}; else it stays markdown.
export function splitBlocks(text, img = null) {
  const lines = String(text ?? "").split("\n");
  const out = []; let md = [];
  const flush = () => { if (md.length && md.join("\n").trim()) out.push({ k: "md", text: md.join("\n") }); md = []; };
  for (let i = 0; i < lines.length; i++) {
    const f = OPEN.exec(lines[i]) ?? TILDE_OPEN.exec(lines[i]);
    if (f) {
      flush();
      const body = []; i++;
      while (i < lines.length && !closes(lines[i], f[1])) body.push(lines[i++]);
      out.push({ k: "code", lang: f[2] || "", text: body.join("\n"), lines: body });
      continue;
    }
    if (tableStarts(lines, i)) {
      flush();
      const head = tableCells(lines[i]);
      const align = tableCells(lines[i + 1]).map(alignOf);
      const rows = []; i += 2;
      while (i < lines.length && /^\s*\|/.test(lines[i])) rows.push(tableCells(lines[i++]));
      i--;
      out.push({ k: "table", head, align, rows });
      continue;
    }
    const m = img ? IMAGE_LINE.exec(lines[i]) : null;
    const drawn = m ? img(imageSrc(m)) : null;
    if (drawn) {
      flush();
      out.push({ k: "img", alt: m[1], src: imageSrc(m), img: drawn });
      continue;
    }
    md.push(lines[i]);
  }
  flush();
  return out;
}

// The image lines of a text outside code and tables, in order: [{alt, src}].
export function imageLines(text) {
  const out = [];
  for (const b of splitBlocks(text)) {
    if (b.k !== "md") continue;
    for (const l of b.text.split("\n")) {
      const m = IMAGE_LINE.exec(l);
      if (m) out.push({ alt: m[1], src: imageSrc(m) });
    }
  }
  return out;
}

// A markdown block cut at its paragraphs and top-level list items, for "Inline PR icons" (#68).
// [{k:"para"|"item"|"other", lines, tail, mark}]: `lines` the piece's source lines, `tail` the blank lines
// after it, `mark` an item's "-" or "3.". A heading, quote or rule line is "other"; an item with an indented
// line (a nested list or a code line) is "other" too, so it stays TextView. The pieces' lines, with their
// tails, give the block again.
const ITEM = /^([-*+]|\d+[.)])\s+/;
const NESTED = /^\s+([-*+]|\d+[.)])\s/;
const OTHER = /^\s*(#{1,6}\s|>|([-*_=])(\s*\2){2,}\s*$)/;
export function splitPieces(md) {
  const out = [];
  let cur = null;
  const start = (p) => { out.push(p); cur = p; };
  for (const line of String(md).split("\n")) {
    if (!line.trim()) { if (cur) cur.tail.push(line); else start({ k: "other", lines: [], tail: [line] }); continue; }
    const open = !!cur && !cur.tail.length;
    const m = ITEM.exec(line);
    if (m) { start({ k: "item", item: true, lines: [line], tail: [], mark: m[1], body: [line.slice(m[0].length)] }); continue; }
    if (OTHER.test(line)) { start({ k: "other", lines: [line], tail: [], heading: /^\s*#/.test(line) }); continue; }
    // An indented line under an item: a soft break (joined), or a nested list or a later paragraph of it
    // (then the whole item stays TextView).
    if (/^\s/.test(line) && cur?.item) {
      if (!open || NESTED.test(line)) cur.k = "other";
      cur.lines.push(...cur.tail, line); cur.tail = []; cur.body.push(line.trim()); continue;
    }
    // A line right under a paragraph, item or quote joins it (markdown's lazy continuation).
    if (open && !cur.heading && cur.lines.length) { cur.lines.push(line); cur.body?.push(line.trim()); continue; }
    start({ k: "para", lines: [line], tail: [], body: [line] });
  }
  for (const p of out) delete p.heading;
  return out;
}
