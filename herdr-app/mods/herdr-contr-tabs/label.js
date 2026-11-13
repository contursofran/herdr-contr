// A tab's short name. clean(), telling(), leadPanes() and title() are copies of those in
// mods/herdr-contr-spaces/projects.js (a mod cannot import another mod); a test keeps them equal.

const clean = (s) => (s ?? "").replace(/^[^\p{L}\p{N}]+/u, "").trim();

// A title that says something: not empty, not the program's name or key, not a path, not user@host.
function telling(raw, program) {
  const live = clean(raw);
  // clean() would strip a path's leading ~ or /: test the title with only its status glyph gone.
  const path = /^[~/]|^[A-Za-z]:\\/.test((raw ?? "").replace(/^[^\p{L}\p{N}~/]+/u, ""));
  return live && live !== program?.name && !path && !live.includes("@") && live.toLowerCase() !== program?.key ? live : "";
}

// Each tab's lead pane: its first agent, else its first focused terminal, else its first terminal.
export function leadPanes(all) {
  const leads = new Map();
  for (const p of all) {
    if (!p.terminal) continue;
    const current = leads.get(p.tab);
    if (!current || (!current.program?.agent && (p.program?.agent || (!current.focused && p.focused)))) leads.set(p.tab, p);
  }
  return leads;
}

function title(tab, l) {
  const live = telling(l?.title, l?.program);
  return (tab.name === l?.program?.name || !tab.named) && live ? live : tab.name;
}

// A tab's automatic name: its number ("3", "tb3") or its terminal's address ("t2n9").
const AUTO = /^(tb)?\d+$|^t\d+[a-z0-9]{1,4}$/;
// The last part of a folder: "/a/b/contr/" → "contr"; "" for none or the root.
const folderName = (cwd) => String(cwd ?? "").replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "";

// The name the owner gave the tab, else its lead's title (as the sidebar cleans it), else for a plain terminal
// its folder's name (#139), else the program's name, else "Terminal". `lead`: the tab's pane from leadPanes().
export function tabLabel(tab, lead) {
  const auto = AUTO.test(tab.name ?? "");
  const l = title(auto ? { ...tab, named: false } : tab, lead) ?? "";
  if (!AUTO.test(l) && l) return l;
  return (!lead?.program?.agent && folderName(lead?.cwd)) || lead?.program?.name || l || "Terminal";
}
