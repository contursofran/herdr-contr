// herdr-contr settings: <appHome>/herdr-contr/config.json, set in the herdr-contr settings dialog.
// {"status_row": "show", "remote_helper": "ask", "text_size": "default"}. A missing file or key is the default;
// the install never creates the file. A file that is not JSON is the defaults too, and is left alone
// (the screen says so). The helper reads the same file (the mod passes --home) for
// remote_helper. Kept in shared(), so every copy of this mod redraws when one changes it.
import { appHome, readTextFile, writeTextFile, shared, setShared, notify } from "herdr";

export const KEY_CONFIG = "herdr-contr.config";    // { file, bad, text }: the parsed file, and the text it was read from
export const DEFAULTS = { status_row: "show", remote_helper: "ask", phone_read_only: false, text_size: "default" };
export const CHOICES = { status_row: ["show", "hide"], remote_helper: ["ask", "never"], phone_read_only: [false, true], text_size: ["small", "default", "large"] };
// The chat's text size (text_size) over the default 16 px: messages, code boxes and tables follow it.
const TEXT_PX = { small: 14, default: 16, large: 18 };
export const textScale = () => TEXT_PX[setting("text_size")] / 16;
const TEXT_NAME = { small: "Small", default: "Default", large: "Large" };
// The text-smaller / text-larger / text-default keys: one step along Small, Default, Large (no wrap).
export async function stepTextSize(c) {
  const order = CHOICES.text_size, i = order.indexOf(setting("text_size"));
  const to = c === "text-default" ? "default" : order[Math.max(0, Math.min(order.length - 1, i + (c === "text-larger" ? 1 : -1)))];
  if (to === order[i]) return false;
  const ok = await setSetting("text_size", to);
  if (ok) notify({ title: `Chat text: ${TEXT_NAME[to]}`, level: "info", timeout: 1500 });
  return ok;
}
export const configPath = () => `${appHome()}/herdr-contr/config.json`;

export function parseConfig(text) {
  if (text == null) return { file: {}, bad: false, text: null };
  try {
    const v = JSON.parse(text);
    if (v && typeof v === "object" && !Array.isArray(v)) return { file: v, bad: false, text };
  } catch { /* not JSON */ }
  return { file: {}, bad: true, text };
}

export function setting(key) {
  const v = shared(KEY_CONFIG)?.file?.[key];
  return CHOICES[key].includes(v) ? v : DEFAULTS[key];
}
export const configBad = () => shared(KEY_CONFIG)?.bad === true;

// Reads the file again (someone may have edited it); shares it only when it changed.
export async function loadConfig() {
  let text = null;
  try { text = await readTextFile(configPath()); } catch { text = null; }
  const c = parseConfig(text);
  const was = shared(KEY_CONFIG);
  if (!was || was.text !== c.text) setShared(KEY_CONFIG, c);
  return c;
}

// Sets one key and keeps the others. False when the file is not JSON (it stays as it is) or the write failed.
export async function setSetting(key, value) {
  for (let i = 0; i < 2; i++) {
    const c = await loadConfig();
    if (c.bad) return false;
    const file = { ...c.file, [key]: value };
    const text = JSON.stringify(file, null, 2) + "\n";
    // Against the text just read: a change made meanwhile is read again, not overwritten.
    try { await writeTextFile(configPath(), text, c.text); }
    catch { continue; }
    setShared(KEY_CONFIG, { file, bad: false, text });
    return true;
  }
  return false;
}
