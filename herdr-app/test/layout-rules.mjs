// Layout rules of the mod runtime that the stub cannot paint (#149).
const SCROLL = ["overflow_y_scrollbar", "overflow_y_scroll"];
const walk = (el, fn, path = []) => {
  if (!el || typeof el !== "object" || !el._a) return;
  const p = [...path, el._a.id ?? el.tag];
  fn(el, p);
  for (const k of el.kids ?? []) walk(k, fn, p);
};
// Rule 1: each direct child of a scrolling column has flex_none, or the rows shrink and overlap (#137).
// Gives the id path of each child that breaks the rule.
export function scrollBreaks(root) {
  const out = [];
  walk(root, (el, p) => {
    if (el.tag !== "v" || !SCROLL.some((k) => k in el._a)) return;
    (el.kids ?? []).forEach((k, i) => {
      if (k && typeof k === "object" && k._a && !("flex_none" in k._a)) out.push([...p, k._a.id ?? `${k.tag}[${i}]`].join(" > "));
    });
  });
  return out;
}
// Counts the scrolling columns, so that a test sees that it walked one.
export function scrollColumns(root) {
  let n = 0;
  walk(root, (el) => { if (el.tag === "v" && SCROLL.some((k) => k in el._a)) n++; });
  return n;
}
// The ids of all elements, depth first, with the elements.
export function all(root) {
  const out = [];
  walk(root, (el) => out.push(el));
  return out;
}
