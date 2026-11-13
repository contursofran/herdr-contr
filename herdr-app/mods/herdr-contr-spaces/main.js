// Adapted from Herdr's spaces mod (Apache-2.0); changes: the project's favicon, else a folder icon, as the space icon.
// Also: it is the Projects list. Under each project, its agents (from our copy of Herdr's agents mod), an
// orchestrator's workers under it; the projects with no agent go into one "N more projects" row (projects.js).
// Workspace spaces: stable colored avatars, worktrees under their checkout,
// grouped by machine when more than one is known. After a space's name, the repo
// its first terminal is in now, only where unusual: branch, ahead/behind. Agent state lives in the agents list, not here. Click to
// switch, right-click for its menu, drag to reorder.

import { View, div, svg, image } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { spaces, machines, panes, agents, terminals, focused, setAgentOrder, run, openOverlay, exec, hc, theme, shared, setShared, watchFolder, window, layout } from "herdr";
import { bySpace, split, countLine, moreLine, workerText, leadPanes, agentLabel, duration } from "./projects.js";

// The braille loader, as herdr-contr's loader() in mods/herdr-contr/state.js (a mod cannot import another mod).
const BRAILLE = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const loader = (size) => div().flex_none().w(size).h(size).flex().items_center().justify_center()
  .font_family("Menlo").text_size(size).line_height(1).frames(BRAILLE, 80).child(BRAILLE[0]);

// The agent's icon from the app's program table: a one-color glyph tinted by its state, or a picture.
function agentIcon(p, size, color) {
  if (!p?.icon) return div().w(size).flex_none();
  return p.tint ? svg(p.icon).size(size).flex_none().text_color(color) : image(p.icon).w(size).h(size).flex_none();
}
// The left edge of a project's icon, and of its agents' icons: past the chevron.
const INDENT = 22;

function find(node, id) {
  if (node?.id === id) return node;
  for (const child of node?.children ?? []) {
    const found = find(child, id);
    if (found) return found;
  }
  return null;
}

// The sidebar's rail goes by its width, not a flag: dragging its edge narrows it too.
const narrow = () => (find(layout(), "sidebar")?.size?.px ?? Infinity) < 160;

// Where a space is now: its first terminal's live folder, else where it was made.
// `ps`: panes(), read once by the caller (each read copies every pane).
function place(s, ps) {
  const tabs = new Set((s.tabs ?? []).map((c) => c.id));
  return ps.find((p) => tabs.has(p.tab) && p.cwd)?.cwd || s.folder;
}
const rowHeight = ({ tree }) => (tree ? 28 : 30);
const RAIL_CELL = 32;
const railCell = (el) => h_flex().flex_none().justify_center().child(el);
const where = (s, ps) => JSON.stringify([s.machine ?? null, place(s, ps)]);
const base = (path) => path.replace(/\/+$/, "").split("/").pop();
// Changes made outside Herdr (an editor's save, a commit in another app): looked
// for this often, in repos the list shows, while the window is in front.
const FALLBACK = 60000;

// A space speaks up only when something in it needs you.
export function glyph(status) {
  return status === "blocked" || status === "done" ? "●" : "·";
}

export function tone(t, status) {
  return t[status] ?? t.unknown;
}

// Where a project keeps its favicon, in the order looked at; the first file found wins. No .ico:
// Herdr's image() draws SVG, PNG, JPEG and the other GPUI formats, and does not name ICO.
const DIRS = ["", "public/", "static/", "app/", "src/app/", "assets/", "web/"];
export const FAVICONS = [
  ...DIRS.flatMap((d) => ["favicon.svg", "favicon.png"].map((f) => d + f)),
  ...DIRS.flatMap((d) => ["apple-touch-icon.png", "icon.png", "icon.svg"].map((f) => d + f)),
];

// The first favicon file in `folder` (this computer), as an absolute path, or null. One exec, off the UI thread.
export async function findFavicon(folder) {
  const r = await exec("sh", ["-c", 'for f; do [ -f "$f" ] && { printf %s "$f"; exit 0; }; done; exit 1', "sh", ...FAVICONS], { cwd: folder });
  return r.code === 0 && r.stdout ? `${folder.replace(/\/+$/, "")}/${r.stdout}` : null;
}

export default class Spaces extends View {
  init(_props, cx) {
    this.git = {}; // where() -> { name, branch, ahead, behind, repo, linked }, null: not a repo
    this.checked = {}; // where() -> when it was last looked at
    this.unsure = new Set(); // where()s git couldn't answer for (a machine away)
    this.pending = new Set(); // where()s to look at next
    this.busy = {}; // pane -> an agent working or a program in front of its shell
    this.ready = {}; // machine -> its layout was readable at the last look
    this.at = {}; // space -> its where() at the last look
    this.watching = new Set(); // local repos' git folders
    this.icons = {}; // local folder -> its favicon's path, null: none (looked for once per folder)
    // Spaces made while this runs wait (at most a moment) for their git, so a
    // worktree appears under its repo instead of jumping there.
    this.shown = new Set(spaces().map((s) => s.id));
    this.born = {};
    this.pressed = null;
    this.over = null;
    this.machines = [];
    this.reconnecting = JSON.parse(localStorage.getItem("machines.reconnecting") ?? "{}");
    for (const state of Object.values(this.reconnecting)) state.busy = false;
    this.folded = new Set(JSON.parse(localStorage.getItem("machines.folded") ?? "[]"));
    this.closed = new Set(JSON.parse(localStorage.getItem("projects.closed") ?? "[]")); // projects folded shut
    this.more = new Set(); // machines whose "N more projects" row is open
    // Each agent's last state and when it changed (ms), kept across restarts.
    this.changed = JSON.parse(localStorage.getItem("changed") ?? "{}");
    cx.timer.every(30000, (cx) => cx.notify());
    this.refreshMachines(cx);
    cx.timer.every(15000, cx => this.refreshMachines(cx));
    // Refresh only when the refs could have changed (see `due`).
    cx.timer.every(250, (cx) => this.checkNew(cx));
  }

  checkNew(cx) {
    const now = Date.now();
    const sp = spaces();
    for (const s of sp) if (!this.shown.has(s.id)) this.born[s.id] ??= now;
    const due = this.due(now, sp, panes());
    if (due.size) this.refreshGit(due, cx);
    this.findIcons(sp, cx);
    if (sp.some((s) => !this.shown.has(s.id)) || this.orderingDue(now)) cx.notify();
  }

  // Places whose git could have changed: new ones (a new space, a cd), where a
  // command or an agent's turn just ended (a checkout, a commit, edits), those on
  // a machine that is back, and repos in view whose fallback is due.
  due(now, sp, ps) {
    // Moved there: what it knew of that place may be old.
    const at = {};
    for (const s of sp) at[s.id] = where(s, ps);
    const keys = new Set(Object.values(at).filter((k) => JSON.parse(k)[1]));
    const due = new Set([...keys].filter((k) => !(k in this.git)));
    for (const [id, k] of Object.entries(at)) if (this.at[id] && this.at[id] !== k && keys.has(k)) due.add(k);
    this.at = at;
    const spaceOf = {};
    for (const s of sp) for (const t of s.tabs ?? []) spaceOf[t.id] = s;
    const busy = {};
    for (const p of ps) {
      if (!p.terminal || !spaceOf[p.tab]) continue;
      busy[p.pane] = p.status === "working" || !(p.program?.shell || p.program?.agent);
      if (this.busy[p.pane] && !busy[p.pane]) this.sameRepo(at[spaceOf[p.tab].id], keys).forEach((k) => due.add(k));
    }
    this.busy = busy;
    for (const m of machines()) {
      const ready = m.layout === "ready";
      if (ready && this.ready[m.name] === false) for (const k of keys) if (JSON.parse(k)[0] === m.name) due.add(k);
      this.ready[m.name] = ready;
    }
    if (window().active) {
      for (const k of keys) {
        const asks = this.git[k] || this.unsure.has(k);
        if (asks && !this.folded.has(JSON.parse(k)[0]) && now - (this.checked[k] ?? 0) > FALLBACK) due.add(k);
      }
    }
    return due;
  }

  // Looks for the favicon of each local space's folder not looked at yet.
  findIcons(sp, cx) {
    const ps = panes();
    for (const s of sp) {
      const folder = s.machine ? null : place(s, ps);
      if (!folder || folder in this.icons) continue;
      this.icons[folder] = null;
      cx.spawn(async (cx) => {
        try { this.icons[folder] = await findFavicon(folder); } catch { this.icons[folder] = null; }
        if (this.icons[folder]) cx.notify();
      });
    }
  }

  // The space's icon: its project's favicon, else a theme-tinted folder.
  spaceIcon(w, t) {
    const path = w.machine ? null : this.icons[place(w, panes())];
    return path
      ? image(path).size(16).flex_none().rounded(4)
      : h_flex().size(16).flex_none().items_center().justify_center().child(svg("mods/workspace-chrome/folder.svg").size(15).text_color(t.muted));
  }

  // `key` and the other places in its repo (a commit there moves their ahead/behind).
  sameRepo(key, keys) {
    const repo = this.git[key]?.repo;
    return [key, ...(repo ? [...keys].filter((k) => this.git[k]?.repo === repo) : [])];
  }

  refreshMachines(cx) {
    cx.spawn(async cx => {
      try { const r = await hc(["machine", "ls", "--json"]); if (r.code === 0) this.machines = (JSON.parse(r.stdout).machines ?? []).filter((m) => !m.temporary); } catch {}
      cx.notify();
    });
  }

  rebuilt(machine) {
    return machine?.state === "unreachable" && /^a different core answers there \("[^"]+"\)$/.test(machine.why ?? "");
  }

  reconnect(machine, cx) {
    const name = machine.name;
    if (this.reconnecting[name]?.busy) return;
    const state = this.reconnecting[name] ??= { machine, removed: false };
    state.busy = true;
    state.error = null;
    localStorage.setItem("machines.reconnecting", JSON.stringify(this.reconnecting));
    cx.notify();
    cx.spawn(async cx => {
      try {
        const ask = async args => {
          const r = await hc([...args, "--json"]);
          if (r.code !== 0) {
            let error;
            try { error = JSON.parse(r.stdout || r.stderr).error?.message; } catch {}
            throw new Error(error || r.stderr || r.stdout || "Couldn't connect.");
          }
        };
        if (!state.removed) {
          await ask(["machine", "rm", name]);
          state.removed = true;
          localStorage.setItem("machines.reconnecting", JSON.stringify(this.reconnecting));
        }
        const args = ["machine", "add"];
        if (machine.via) args.push("--via", machine.target);
        else args.push(machine.target);
        args.push("--name", name);
        if (machine.session) args.push("--session", machine.session);
        if (machine.temporary) args.push("--temp");
        await ask(args);
        this.folded.delete(name);
        localStorage.setItem("machines.folded", JSON.stringify([...this.folded]));
        delete this.reconnecting[name];
      } catch (e) {
        state.error = String(e.message ?? e);
        state.busy = false;
      }
      localStorage.setItem("machines.reconnecting", JSON.stringify(this.reconnecting));
      this.refreshMachines(cx);
      cx.notify();
    });
  }

  // One look at a time, each place once however often it was asked for meanwhile.
  refreshGit(keys, cx) {
    for (const k of keys) this.pending.add(k);
    if (this.refreshing) return;
    this.refreshing = true;
    cx.spawn(async (cx) => {
      while (this.pending.size) {
        const keys = [...this.pending];
        this.pending.clear();
        await Promise.all(keys.map((key) => this.check(key, cx)));
      }
      const used = new Set(Object.values(this.git).filter(Boolean).map((g) => JSON.parse(g.repo)).filter(([on]) => !on).map(([, repo]) => repo));
      for (const repo of this.watching) if (!used.has(repo)) this.watching.delete(repo);
      this.refreshing = false;
      cx.notify();
    });
  }

  // One place's git: branch, ahead/behind, its repo.
  async check(key, cx) {
    const [on, cwd] = JSON.parse(key);
    this.checked[key] = Date.now();
    this.unsure.delete(key);
    try {
      // Ref-only reads avoid scanning the worktree or waking its index watch.
      // Two reads: a folder name may itself contain a newline.
      const [r, common] = await Promise.all([
        exec("git", ["rev-parse", "--path-format=absolute", "--show-toplevel"], { cwd, on }),
        exec("git", ["rev-parse", "--path-format=absolute", "--git-common-dir"], { cwd, on }),
      ]);
      if (r.code !== 0 || common.code !== 0) {
        // Only git's own "no" is remembered as no repo; anything else is asked again.
        if (!/not a git repository/i.test(r.code !== 0 ? r.stderr : common.stderr)) this.unsure.add(key);
        this.git[key] = null;
        return;
      }
      // The repo's own .git folder names it; a worktree's top differs from it.
      const top = r.stdout.replace(/\n$/, ""), repo = common.stdout.replace(/\n$/, "");
      const [branch, counts] = await Promise.all([
        exec("git", ["symbolic-ref", "-q", "HEAD"], { cwd, on }),
        exec("git", ["rev-list", "--left-right", "--count", "HEAD...@{upstream}"], { cwd, on }),
      ]);
      const name = repo.endsWith("/.git") ? base(repo.slice(0, -5)) : base(repo).replace(/\.git$/, "");
      const [ahead, behind] = counts.code === 0 ? counts.stdout.trim().split(/\s+/).map(Number) : [0, 0];
      const info = { name, branch: branch.code === 0 ? branch.stdout.trim().replace(/^refs\/heads\//, "") : "(detached)", ahead, behind, repo: JSON.stringify([on, repo]), linked: repo !== `${top}/.git` };
      this.git[key] = info;
      if (!on) this.watch(repo, cx);
    } catch {
      // Couldn't ask (the machine is away): keep what it showed, look again later.
      this.git[key] ??= null;
      this.unsure.add(key);
    }
  }

  watch(repo, cx) {
    if (this.watching.has(repo)) return;
    this.watching.add(repo);
    cx.spawn(async (cx) => {
      try {
        while (this.watching.has(repo)) {
          await watchFolder(repo);
          const mine = JSON.stringify([null, repo]);
          if (this.watching.has(repo)) this.refreshGit(Object.keys(this.git).filter((k) => this.git[k]?.repo === mine), cx);
        }
      } catch { this.watching.delete(repo); }
    });
  }

  menu(w) {
    // A popup is as tall as its items; height is only the most.
    openOverlay({ mod: "menu", kind: "popup", width: 200, height: 260, props: { tab: "space", target: { id: w.id } } });
  }

  orderingDue(now) {
    return this.ordering && now - this.ordering.at >= 1000
      && this.ordering.machines.every(name => machines().some(m => m.name === name && m.layout === "ready"));
  }

  // Spaces in order, each worktree space moved under its main checkout's.
  ordered() {
    const sp = spaces(), ps = panes();
    const ready = (w) => this.shown.has(w.id) || where(w, ps) in this.git || Date.now() - (this.born[w.id] ??= Date.now()) > 1500;
    for (const w of sp) if (!this.shown.has(w.id) && ready(w)) this.shown.add(w.id);
    const list = sp.map((w, i) => ({ w, i, g: this.git[where(w, ps)] ?? null }));
    const mains = new Set(list.filter((r) => r.g && !r.g.linked).map((r) => r.g.repo));
    const nested = (r) => r.g?.linked && mains.has(r.g.repo);
    const out = [];
    const placed = new Set(); // repos whose worktrees are listed (under the first of its spaces)
    for (const r of list) {
      if (nested(r)) continue;
      out.push({ ...r, tree: null });
      if (!r.g || r.g.linked || placed.has(r.g.repo)) continue;
      placed.add(r.g.repo);
      const kids = list.filter((k) => nested(k) && k.g.repo === r.g.repo);
      kids.forEach((k, n) => out.push({ ...k, tree: n === kids.length - 1 ? "└─" : "├─" }));
    }
    // Simulate the batch so later indexes include earlier moves. An attempted
    // write can lose a sync conflict: allow the echo to settle before retrying.
    const order = sp.map(w => w.id);
    const target = out.map(r => r.w.id);
    const key = JSON.stringify([order, target, machines()]);
    if (order.every((id, i) => id === target[i])) {
      this.ordering = null;
    } else if (this.ordering?.key !== key || this.orderingDue(Date.now())) {
      const moved = new Set();
      this.ordering = { key, at: Date.now(), machines: [] };
      for (let i = 0; i < out.length; i++) {
        const from = order.indexOf(out[i].w.id);
        if (from === i) continue;
        run("space.move", { id: out[i].w.id, index: i + 1 });
        moved.add(out[i].w.machine ?? null);
        order.splice(i, 0, order.splice(from, 1)[0]);
      }
      this.ordering.machines = [...moved];
    }
    return out.filter(r => this.shown.has(r.w.id));
  }

  // The rows, under a header per machine once one isn't this computer.
  grouped(t, collapsed) {
    const rows = this.ordered();
    // Herdr's agent keys go in the order of the projects.
    setAgentOrder(rows.flatMap((r) => this.agentsOf.get(r.w.id) ?? []).map((x) => x.a.address));
    const draw = (r) => (collapsed ? this.compact(r, t) : this.row(r, t));
    const list = (rs, m) => (collapsed ? rs.map(draw) : this.projects(rs, m, t));
    const remote = [...new Set([...rows.map(r => r.w.machine).filter(Boolean), ...this.machines.map(m => m.name), ...Object.keys(this.reconnecting)])].sort();
    const loading = m => {
      const layout = machines().find(k => k.name === m)?.layout;
      return layout === "loading" || (!layout && this.machines.some(k => k.name === m && k.state === "connected"))
        || spaces().some(s => (s.machine ?? null) === m && !this.shown.has(s.id));
    };
    const loadingRow = m => h_flex().id(`machine-loading-${m ?? "local"}`).h(28).flex_none().px(collapsed ? 0 : 8).items_center().when(collapsed, el => el.justify_center()).text_size(12).text_color(t.dim).child(collapsed ? "…" : "Loading…");
    if (!remote.length) return loading(null) && !rows.length ? [loadingRow(null)] : list(rows, null);
    const machineNames = [null, ...remote];
    return machineNames.flatMap((m) => {
      const mine = rows.filter((r) => (r.w.machine ?? null) === m);
      const repair = this.reconnecting[m];
      const machine = this.machines.find(k => k.name === m) ?? repair?.machine;
      const connected = !m || machine?.state === "connected";
      const rebuilt = this.rebuilt(machine) || !!repair;
      const login = machine?.state === "needs_login" ? `hc machine login ${m}` : null;
      const folded = this.folded.has(m);
      const icon = svg(`mods/workspace-chrome/${m ? "server" : "monitor"}.svg`).size(13).flex_none();
      const badge = div().absolute().right(-3).bottom(-2).size(6).rounded(3).border_1().border_color(t.chrome).bg(connected ? t.ansi2 : t.dim);
      const head = h_flex().id(`machine-${m ?? "local"}`).h(26).flex_none().px(collapsed ? 0 : 8).gap(7).items_center().text_size(11).text_color(t.dim)
        .when(collapsed, el => el.mt(4).w(RAIL_CELL).justify_center().rounded(6).tooltip(login ? `${m} needs a login: run ${login}. Click for Copy.` : m ? `${m} · ${connected ? "connected" : (machine?.state ?? "asleep").replace(/_/g, " ")}` : "This computer"))
        .cursor_pointer().hover(s => s.bg(t.hover))
        .on_click((_e, cx) => {
          if (collapsed && login) { run("app.settings", { section: "machines" }); return; }
          folded ? this.folded.delete(m) : this.folded.add(m);
          localStorage.setItem("machines.folded", JSON.stringify([...this.folded]));
          cx.notify();
        })
        .when(!collapsed, el => el.child(icon))
        .when(collapsed, el => el.child(div().relative().flex_none().child(icon).when(Boolean(m), el => el.child(badge))))
        .when(!collapsed, el => el.child(div().flex_1().truncate().child(m ?? "This computer"))
          .when(!connected && !rebuilt && !login, el => el.child(div().flex_none().child((machine?.state ?? "asleep").replace(/_/g, " "))))
          .when(rebuilt, el => el.child(h_flex().flex_none().gap(3).items_center()
            .child(div().max_w(140).truncate().child(repair?.error || "rebuilt ·"))
            .child(div().id(`machine-connect-${m}`).text_color(repair?.error ? t.blocked : t.muted)
              .hover(s => s.text_color(t.foreground))
              .tooltip(repair?.error || "This machine was rebuilt or replaced. Click to connect to the new one.")
              .on_click((_e, cx) => { cx.stop_propagation(); this.reconnect(machine, cx); })
              .child(repair?.busy ? "Connecting…" : "Connect"))))
          .when(connected, el => el.child(div().id(`machine-add-${m ?? "local"}`).flex_none().px(4).rounded(4).hover(s => s.bg(t.hover).text_color(t.foreground))
            .on_click((_e, cx) => { cx.stop_propagation(); run("space.new", { machine: m ?? "" }); }).child("+")))
          .child(div().size(6).rounded(3).flex_none().bg(connected ? t.ansi2 : t.dim))
          .child(svg(`mods/workspace-chrome/chevron-${folded ? "right" : "down"}.svg`).size(11).flex_none()));
      const empty = h_flex().id(`machine-new-${m ?? "local"}`).h(28).flex_none().px(collapsed ? 0 : 8).gap(7).items_center().rounded(7)
        .text_size(12).text_color(t.dim).hover(s => s.bg(t.hover).text_color(t.foreground))
        .on_click(() => run("space.new", { machine: m ?? "" }))
        .when(collapsed, el => el.w(RAIL_CELL).rounded(6).justify_center().tooltip(`New space on ${m ?? "this computer"}`)
          .child(h_flex().size(18).rounded(5).border_1().border_color(t.dim).items_center().justify_center()
            .child(svg("mods/workspace-chrome/plus.svg").size(10))))
        .when(!collapsed, el => el.child(svg("mods/workspace-chrome/plus.svg").size(12)).child("New space there"));
      if (!collapsed && rebuilt) return [head];
      const cell = collapsed ? railCell : (el) => el;
      const guidance = !collapsed && login ? [v_flex().px(8).pb(6).gap(3).flex_none()
        .child(div().text_size(11).text_color(t.blocked).child("needs a login: run"))
        .child(h_flex().gap(5).items_center()
          .child(div().flex_1().min_w(0).truncate().tooltip(login).text_size(11).text_color(t.muted).child(login))
          .child(div().id(`machine-login-copy-${m}`).flex_none().px(5).py(2).rounded(4)
            .text_size(11).text_color(t.foreground).bg(t.raised).cursor_pointer().hover(s => s.bg(t.hover))
            .on_click((_e, cx) => { cx.stop_propagation(); cx.write_to_clipboard(login); this.copiedLogin = login; cx.notify(); })
            .child(this.copiedLogin === login ? "Copied" : "Copy")))] : [];
      return [cell(head), ...guidance, ...(folded ? [] : mine.length ? list(mine, m) : [loading(m) ? loadingRow(m) : cell(empty)])];
    });
  }

  // After the name: the branch (mono 11, muted); at the right, on a shut project its agents' state marks,
  // then the commits ahead and behind ("\u21934", mono 11, muted).
  gitTail(w, g, t) {
    const shut = this.closed.has(w.id) ? this.agentsOf.get(w.id) ?? [] : [];
    return h_flex()
      .flex_1()
      .min_w(0)
      .gap(6)
      .items_center()
      .font_family("Menlo")
      .text_size(11)
      .text_color(t.muted)
      .child(div().flex_1().min_w(0).truncate().child(g?.branch ?? ""))
      .children(shut.map((x) => this.mark(t, x.a.status, 13)))
      .when(g?.ahead > 0, (el) => el.child(div().flex_none().child(`\u2191${g.ahead}`)))
      .when(g?.behind > 0, (el) => el.child(div().flex_none().child(`\u2193${g.behind}`)));
  }

  row({ w, i, g, tree }, t) {
    // The shown space: its selected agent row says it, else the project row.
    const on = w.active && !(this.agentsOf.has(w.id) && !this.closed.has(w.id));
    const fromIndex = this.pressed === null ? -1 : spaces().findIndex(s => s.id === this.pressed);
    return v_flex()
      .relative()
      .id(`space-${w.id}`)
      .pl(6)
      .pr(10)
      .h(rowHeight({ tree }))
      .flex_none()
      .justify_center()
      .rounded(8)
      .when(on, (el) => el.bg(t.selected))
      .when(!on, (el) => el.hover((s) => s.bg(t.hover)))
      .when(this.pressed === w.id && this.over !== w.id, (el) => el.opacity(0.5))
      .when(this.pressed !== null && this.over === w.id && this.pressed !== w.id, (el) => el.child(
        div().absolute().left(0).right(0).h(2).bg(t.accent)
          .when(fromIndex < i, line => line.bottom(0))
          .when(fromIndex > i, line => line.top(0)),
      ))
      .on_mouse_down("left", () => {
        this.pressed = w.id;
        this.over = w.id;
      })
      .on_mouse_move((_e, cx) => {
        const source = spaces().find(s => s.id === this.pressed);
        const over = source && (source.machine ?? null) === (w.machine ?? null) ? w.id : null;
        if (this.pressed !== null && this.over !== over) {
          this.over = over;
          cx.notify();
        }
      })
      .on_mouse_up("left", (_e, cx) => {
        const from = this.pressed;
        const over = this.over;
        this.pressed = null;
        this.over = null;
        if (from !== null && over === w.id && from !== w.id) {
          const index = spaces().findIndex(s => s.id === w.id);
          if (index !== -1) run("space.move", { id: from, index: index + 1 });
        } else if (from === w.id) run("space.select", { id: w.id });
        cx.notify();
      })
      .on_mouse_down("right", () => this.menu(w))
      .child(
        h_flex()
          .gap(6)
          .items_center()
          .child(this.chevron(w, t))
          .when(Boolean(tree), el => el.pl(22).child(svg("mods/workspace-chrome/branch.svg").size(13).text_color(t.dim)))
          .when(!tree, el => el.child(this.spaceIcon(w, t)))
          .child(div().flex_shrink(0).max_w(150).truncate().font_medium().text_color(t.foreground).child(w.name))
          .child(this.gitTail(w, g, t)),
      );
  }

  // A project's chevron: it opens and shuts the project's agents. A press on it is not a drag or a select.
  chevron(w, t) {
    const has = this.agentsOf?.has(w.id);
    const shut = this.closed.has(w.id);
    return div().id(`fold-${w.id}`).size(14).flex_none().flex().items_center().justify_center().text_color(t.dim)
      .when(Boolean(has), (el) => el.rounded(4).hover((s) => s.text_color(t.foreground))
        .on_mouse_down("left", (_e, cx) => {
          cx.stop_propagation();
          shut ? this.closed.delete(w.id) : this.closed.add(w.id);
          localStorage.setItem("projects.closed", JSON.stringify([...this.closed]));
          cx.notify();
        })
        .child(svg(`mods/workspace-chrome/chevron-${shut ? "right" : "down"}.svg`).size(11)));
  }

  // One machine's projects: each with its agents under it, then the "N more projects, no agents" row.
  projects(rows, m, t) {
    const { shown, idle } = split(rows, (id) => this.agentsOf.has(id));
    // A project and its agents, 2 px apart, 10 px under the group.
    const out = shown.map((r) => v_flex().flex_none().gap(2).pb(8).child(this.row(r, t))
      .children(this.closed.has(r.w.id) ? [] : (this.agentsOf.get(r.w.id) ?? []).flatMap((x) =>
        [this.agentRow(x, t), ...(x.workers.length ? [this.workersBox(x.workers, t)] : [])])));
    if (!idle.length) return out;
    const key = m ?? "";
    const open = this.more.has(key);
    const more = h_flex().id(`more-${m ?? "local"}`).h(30).flex_none().pl(6).pr(10).gap(6).items_center().rounded(8)
      .text_size(12).text_color(t.muted).cursor_pointer().hover((s) => s.bg(t.hover).text_color(t.foreground))
      .on_click((_e, cx) => { open ? this.more.delete(key) : this.more.add(key); cx.notify(); })
      .child(div().size(14).flex_none().flex().items_center().justify_center()
        .child(svg(`mods/workspace-chrome/chevron-${open ? "down" : "right"}.svg`).size(11)))
      .child(div().flex_1().min_w(0).truncate().child(moreLine(idle.length)));
    return [...out, more, ...(open ? idle.map((r) => this.row(r, t)) : [])];
  }

  // An agent under its project: its icon (45%, full on the shown row), the title, and at the right the age
  // (when it does not work) and the state mark (when it works or waits). Under the title of an agent with
  // live workers: "Orchestrating · N workers". The shown row is raised, with a 1 px ring; its title is full
  // ink, 500. A finished agent not looked at yet (done) is full ink, 600; any other is muted, 400.
  agentRow({ a, workers }, t) {
    const when = this.changed[a.address]?.at ?? 0;
    const on = a.focused;
    const unread = !on && a.status === "done";
    return v_flex().id(`ag-${a.address}`).py(6).pl(26).pr(10).gap(2).flex_none().rounded(8)
      .when(on, (el) => el.bg(t.raised).border_1().border_color(t.border))
      .when(!on, (el) => el.hover((s) => s.bg(t.hover)))
      .cursor_pointer()
      .on_click(() => run("pane.focus", { pane: a.pane }))
      .child(h_flex().h(18).gap(8).items_center()
        .child(agentIcon(a.program, 15, t[a.status] ?? t.muted).when(!on, (el) => el.opacity(0.45)))
        .child(div().flex_1().min_w(0).truncate().text_size(13)
          .when(on, (el) => el.font_medium().text_color(t.foreground))
          .when(unread, (el) => el.font_semibold().text_color(t.foreground))
          .when(!on && !unread, (el) => el.text_color(t.muted))
          .child(this.labels.get(a.address) ?? ""))
        .when(Boolean(when) && a.status !== "working", (el) => el.child(div().flex_none().font_family("Menlo").text_size(11).text_color(t.muted).child(duration(this.now - when))))
        .when(a.status === "working" || a.status === "blocked", (el) => el.child(this.mark(t, a.status, 15))))
      .when(workers.length > 0, (el) => el.child(h_flex().h(16).gap(8).items_center()
        .child(div().w(15).flex_none())
        .child(div().flex_1().min_w(0).truncate().text_size(12).text_color(t.muted).child(countLine(workers.length)))));
  }

  // An orchestrator's workers: 32 px in, with a 1 px guide line at the left.
  workersBox(list, t) {
    const shown = focused()?.agent;
    return v_flex().flex_none().ml(32).pl(9).gap(2).border_l(1).border_color(t.border)
      .children(list.map(({ t: k, depth }) => {
        const st = this.listed.get(k.address) ?? (k.status === "error" ? "blocked" : k.status);
        const on = k.address === shown;
        return h_flex().id(`sub-${k.address}`).h(26).flex_none().pl(8 + 14 * depth).pr(8).gap(8).items_center().rounded(6)
          .text_size(12)
          .when(on, (el) => el.bg(t.raised))
          .when(!on, (el) => el.hover((s) => s.bg(t.hover)))
          .cursor_pointer()
          .on_click(() => run(this.listed.has(k.address) ? "pane.focus" : "terminal.peek", { terminal: k.address }))
          .child(agentIcon(k.program, 13, t[st] ?? t.muted))
          .child(div().flex_1().min_w(0).truncate().text_color(on ? t.foreground : st === "done" ? t.dim : t.muted).child(workerText(k)))
          .child(this.mark(t, st, 12));
      }));
  }

  // A state mark, mono: the braille loader in full ink while it works, "\u283f" pulsing in the blocked color
  // while it waits for the owner, "\u2836" when idle and "\u2713" when done (muted).
  mark(t, status, size) {
    if (status === "working") return loader(size).text_color(t.foreground);
    const glyph = (c, color) => div().flex_none().font_family("Menlo").text_size(size).line_height(1).text_color(color).child(c);
    if (status === "blocked") return glyph("\u283f", t.blocked).loop("opacity", { from: 0.35, to: 1, period: 1400 });
    if (status === "done") return glyph("\u2713", t.muted);
    return glyph("\u2836", t.muted);
  }

  // Stamps agents whose state changed. Done and idle count as one: looking at a finished agent turns
  // done into idle, and looking isn't the agent doing anything.
  track(list, now) {
    let dirty = false;
    for (const a of list) {
      const s = a.status === "done" ? "idle" : a.status;
      if (this.changed[a.address]?.s === s) continue;
      this.changed[a.address] = { s, at: now };
      dirty = true;
    }
    if (!dirty) return;
    const listed = new Set(list.map((a) => a.address));
    for (const [k, c] of Object.entries(this.changed)) if (!listed.has(k) && now - c.at > 30 * 86400000) delete this.changed[k];
    localStorage.setItem("changed", JSON.stringify(this.changed));
  }

  // The data of one draw: agents by space, their titles, and the time. Read once per draw.
  readAgents() {
    const list = agents();
    this.now = Date.now();
    this.listed = new Map(list.map((a) => [a.address, a.status]));
    this.track(list, this.now);
    this.agentsOf = bySpace(list, terminals());
    const leads = leadPanes(panes());
    const tabs = new Map(spaces().flatMap((s) => s.tabs ?? []).map((tab) => [tab.id, tab]));
    const perTab = new Map();
    for (const a of list) perTab.set(a.tab, (perTab.get(a.tab) ?? 0) + 1);
    this.labels = new Map(list.map((a) => [a.address, agentLabel(a, tabs, leads, perTab.get(a.tab) > 1)]));
  }

  compact({ w, i }, t) {
    return railCell(h_flex()
      .id(`space-${w.id}`)
      .relative()
      .w(RAIL_CELL)
      .h(26)
      .rounded(6)
      .justify_center()
      .items_center()
      .when(w.active, (el) => el.bg(t.selected))
      .when(!w.active, (el) => el.hover((s) => s.bg(t.hover)))
      .on_click(() => run("space.select", { id: w.id }))
      .child(div().text_color(w.active ? t.foreground : t.muted).child(String(i + 1)))
      .child(div().absolute().right(5).text_color(tone(t, w.status)).child(glyph(w.status))));
  }

  render() {
    const t = theme();
    const collapsed = narrow();
    const palette = ["ansi6", "ansi3", "ansi4", "ansi5", "ansi2", "ansi1"];
    const sp = spaces();
    const ids = new Set(sp.map(p => p.id));
    const kept = Object.fromEntries(Object.entries(shared("space.colors") ?? {}).filter(([id]) => ids.has(id)));
    let changed = Object.keys(kept).length !== Object.keys(shared("space.colors") ?? {}).length;
    for (const p of sp) {
      if (kept[p.id] || p.name === "~") continue;
      const uses = c => Object.values(kept).filter(v => v === c).length;
      kept[p.id] = palette.reduce((best, c) => uses(c) < uses(best) ? c : best);
      changed = true;
    }
    if (changed) setShared("space.colors", kept);
    this.readAgents();
    const rows = this.grouped(t, collapsed);
    return v_flex().size_full().min_h(0).bg(t.chrome).text_size(13)
      .child(h_flex().pt(6).pr(10).pb(4).pl(20).flex_none().items_center().justify_between().text_size(12).font_medium().text_color(t.muted)
        .when(collapsed, el => el.pl(0).pr(0).justify_center())
        .when(!collapsed, el => el.child("Projects"))
        .child(div().id("space-new").size(24).flex_none().flex().items_center().justify_center().rounded(8).hover(s => s.bg(t.hover).text_color(t.foreground))
          .on_click(() => run("space.new")).child(svg("mods/workspace-chrome/plus.svg").size(14))))
      .child(v_flex().flex_1().min_h(0).px(collapsed ? 0 : 10).gap(collapsed ? 0 : 2).pb(14).overflow_y_scrollbar().mode("always")
        .on_mouse_up("left", (_e, cx) => { this.pressed = null; cx.notify(); })
        .children(rows));
  }
}
