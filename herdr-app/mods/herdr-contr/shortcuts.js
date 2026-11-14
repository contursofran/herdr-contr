// The keys that the three mods bind, for the settings dialog's Shortcuts section (#165). One table: the reply box
// binds its keys from it (composer.js), and test/keymap.test.mjs compares it with keys.json and with the key
// handlers, so the section cannot differ from the real keys.
// A key is a Herdr keystroke ("cmd-shift-t"); "ctrl-1..9" is a range. `bound`: where the key is bound:
// "keys.json" (with its `action`), "bind_keys" (composer.js, in `context`), "handler" (an on_key_down in `file`)
// or "submit" (the box's own Enter, composer.js).
export const SHORTCUT_GROUPS = ["Window", "Reply box", "Cards", "Settings"];

export const SHORTCUTS = [
  { group: "Window", what: "Switch Chat | Terminal", keys: ["cmd-j", "cmd-shift-t"], bound: "keys.json", action: "herdr-contr.view" },
  { group: "Window", what: "Show the main agent of project 1 to 9, in the sidebar's order", keys: ["ctrl-1..9"], bound: "keys.json", action: "herdr-contr.main-" },
  { group: "Window", what: "Show or hide the sidebar", keys: ["cmd-b"], bound: "keys.json", action: "herdr-contr.sidebar" },
  { group: "Window", what: "Open or close the agent's terminal", keys: ["ctrl-`"], bound: "keys.json", action: "herdr-contr.terminal" },
  { group: "Window", what: "Needs you", keys: ["cmd-shift-i"], bound: "keys.json", action: "herdr-contr.needs" },
  { group: "Window", what: "Overview", keys: ["cmd-shift-\\"], bound: "keys.json", action: "herdr-contr.overview" },
  { group: "Window", what: "herdr-contr settings", keys: ["cmd-shift-,"], bound: "keys.json", action: "herdr-contr.settings" },
  { group: "Window", what: "Previous project", keys: ["cmd-alt-up"], bound: "keys.json", action: "space.prev", file: "chat.js" },
  { group: "Window", what: "Next project", keys: ["cmd-alt-down"], bound: "keys.json", action: "space.next", file: "chat.js" },
  { group: "Window", what: "Move the tab left", keys: ["cmd-ctrl-shift-left"], bound: "keys.json", action: "herdr-contr.move-left" },
  { group: "Window", what: "Move the tab right", keys: ["cmd-ctrl-shift-right"], bound: "keys.json", action: "herdr-contr.move-right" },
  { group: "Window", what: "Chat text smaller", keys: ["cmd-alt--"], bound: "keys.json", action: "herdr-contr.text-smaller" },
  { group: "Window", what: "Chat text larger", keys: ["cmd-alt-="], bound: "keys.json", action: "herdr-contr.text-larger" },
  { group: "Window", what: "Chat text default size", keys: ["cmd-alt-0"], bound: "keys.json", action: "herdr-contr.text-default" },
  { id: "paste", group: "Reply box", what: "Paste an image or a file", keys: ["secondary-v"], bound: "bind_keys", context: "HerdrContrComposer" },
  { id: "slash-up", group: "Reply box", what: "\"/\" list: the command above", keys: ["up"], bound: "bind_keys", context: "HerdrContrSlash" },
  { id: "slash-down", group: "Reply box", what: "\"/\" list: the command below", keys: ["down"], bound: "bind_keys", context: "HerdrContrSlash" },
  { id: "slash-complete", group: "Reply box", what: "\"/\" list: complete the command (Enter also)", keys: ["tab"], bound: "bind_keys", context: "HerdrContrSlash" },
  { id: "slash-close", group: "Reply box", what: "\"/\" list: close", keys: ["escape"], bound: "bind_keys", context: "HerdrContrSlash" },
  { id: "suggest", group: "Reply box", what: "Put the suggested prompt in the box", keys: ["tab"], bound: "bind_keys", context: "HerdrContrSuggest" },
  { group: "Reply box", what: "Send", keys: ["enter"], bound: "submit" },
  { group: "Reply box", what: "New line", keys: ["shift-enter"], bound: "submit" },
  { group: "Cards", what: "Panel card: send the key to the agent's prompt", keys: ["up", "down", "left", "right", "enter", "tab", "space", "escape", "0..9"], bound: "handler", file: "panelcard.js" },
  { group: "Cards", what: "Question card: pick option 1 to 9", keys: ["1..9"], bound: "handler", file: "cards.js" },
  { group: "Cards", what: "Question card: next question, or send the answers", keys: ["enter"], bound: "handler", file: "cards.js" },
  { group: "Cards", what: "Approval card: allow (1) or deny (2)", keys: ["1", "2"], bound: "handler", file: "cards.js" },
  { group: "Settings", what: "Pick the section or the choice that has the focus", keys: ["enter", "space"], bound: "handler", file: "settings.js" },
];

// The keystrokes of a key: "ctrl-1..9" gives ctrl-1 … ctrl-9.
export function expand(key) {
  const m = /^(.*?)(\d)\.\.(\d)$/.exec(key);
  if (!m) return [key];
  const out = [];
  for (let n = Number(m[2]); n <= Number(m[3]); n++) out.push(`${m[1]}${n}`);
  return out;
}

// The keystroke of a row with an id (composer.js binds from it).
export const keystroke = (id) => SHORTCUTS.find((s) => s.id === id).keys[0];

const MAC = { cmd: "⌘", secondary: "⌘", ctrl: "⌃", alt: "⌥", shift: "⇧" };
const OTHER = { cmd: "Ctrl", secondary: "Ctrl", ctrl: "Ctrl", alt: "Alt", shift: "Shift" };
const NAMES = { up: "↑", down: "↓", left: "←", right: "→", enter: "Enter", tab: "Tab", space: "Space", escape: "Esc" };

// The key caps of one keystroke: "cmd-shift-t" is ["⌘", "⇧", "T"] on macOS and ["Ctrl", "Shift", "T"] elsewhere.
export function caps(stroke, platform = "macos") {
  const mods = platform === "macos" ? MAC : OTHER;
  const parts = stroke.split("-");
  // "cmd-alt--": the key is "-".
  const key = stroke.endsWith("--") ? "-" : parts.pop();
  if (key === "-") parts.splice(parts.length - 2);
  const out = parts.map((p) => mods[p] ?? p);
  if (platform !== "macos") for (let i = out.length - 1; i > 0; i--) if (out[i] === out[i - 1]) out.splice(i, 1);
  out.push(NAMES[key] ?? key.toUpperCase());
  return out;
}

// The caps of a key as display groups: a range "ctrl-1..9" is [[⌃,1], "…", [⌃,9]].
export function keyCaps(key, platform) {
  const all = expand(key);
  if (all.length === 1) return [caps(all[0], platform)];
  return [caps(all[0], platform), "…", caps(all[all.length - 1], platform)];
}
