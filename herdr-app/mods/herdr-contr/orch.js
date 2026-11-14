// The orchestration tool (xplat/herdr-contr-orch) that the helper's install puts in the Herdr home. The mod
// runs it with this Herdr home, so the tool uses the files of this home (#186).
import { appHome, exec } from "herdr";

export const orchPath = () => `${appHome()}/herdr-contr/bin/herdr-contr-orch`;

// Runs the tool with `args`. Gives { ok, stdout, error }: `error` is the tool's stderr, or why it did not
// start. It never throws.
export async function runOrch(args, timeout = 30000) {
  let r;
  try { r = await exec("env", [`HERDR_APP_HOME=${appHome()}`, orchPath(), ...args], { timeout }); }
  catch (e) { return { ok: false, code: null, stdout: "", error: `herdr-contr-orch could not start (${e?.message ?? e}).` }; }
  if (r?.code === 0) return { ok: true, code: 0, stdout: String(r.stdout ?? ""), error: null };
  const why = String(r?.stderr ?? "").trim();
  return { ok: false, code: r?.code ?? null, stdout: "", error: why || `herdr-contr-orch ${args.slice(0, 2).join(" ")} failed (exit ${r?.code ?? "?"}).` };
}

// Runs the tool and parses its stdout as JSON. { value } or { error }.
export async function orchJson(args) {
  const r = await runOrch(args);
  if (!r.ok) return { error: r.error };
  try { return { value: JSON.parse(r.stdout) }; }
  catch { return { error: `herdr-contr-orch ${args.slice(0, 2).join(" ")} gave text that is not JSON.` }; }
}
