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

// The name the owner gave the tab, else its lead's title (as the sidebar cleans it), else the
// program's name, else the tab's number. `lead`: the tab's pane from leadPanes(), or undefined.
export function tabLabel(tab, lead) {
  const auto = /^(tb)?\d+$/.test(tab.name ?? "");
  const l = title(auto ? { ...tab, named: false } : tab, lead) ?? "";
  return /^(tb)?\d+$/.test(l) || !l ? lead?.program?.name || l || "Terminal" : l;
}
