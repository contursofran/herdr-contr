// Pull request and issue references in chat prose, as markdown links (#57): "PR #51", "#51",
// "owner/repo#51" and github.com/<owner>/<repo>/pull|issues/<n> URLs. TextView underlines a link and
// opens it in the browser. Not inside code spans, links, URLs or words; "#fff" is not a number.
// A bare "#51" needs the repository of the agent's folder (repoFor); without one it stays text.
// "name #51" with the name of a known repository (any folder read by repoFor) links there (#101).
import { exec } from "herdr";

const GH = "https://github.com";
const URL_RE = /https?:\/\/github\.com\/([\w.-]+)\/([\w.-]+)\/(pull|issues)\/(\d+)(?![\w/])/y;
const SKIP_RE = /\[[^\]\n]*\]\([^)\n]*\)|<[^>\n]*>|https?:\/\/[^\s)]+/y;
const SLUG_RE = /([\w.-]+)\/([\w.-]+)#(\d+)(?![\w-])/y;
const BARE_RE = /(PR #|PR#|#)(\d+)(?![\w-])/y;
// "name #51", "name PR #51", "name#51": the name of a known repository before a bare reference (#101).
const NAMED_RE = /([\w.-]+)( ?)((?:PR #|PR#|#)(\d+))(?![\w-])/y;

const at = (re, s, i) => { re.lastIndex = i; return re.exec(s); };

// One segment without code spans.
function linkPlain(s, repo) {
  let out = "", i = 0, from = 0;
  const put = (end, md) => { out += s.slice(from, i) + md; i = from = end; };
  while (i < s.length) {
    const prev = s[i - 1] ?? " ";
    let m, slug;
    if ((m = at(URL_RE, s, i))) put(i + m[0].length, `[${m[0]}](${m[0]})`);
    else if ((m = at(SKIP_RE, s, i))) i += m[0].length;
    else if (!/[\w/#&.-]/.test(prev) && (m = at(SLUG_RE, s, i))) put(i + m[0].length, `[${m[0]}](${GH}/${m[1]}/${m[2]}/issues/${m[3]})`);
    else if (!/[\w/#&.-]/.test(prev) && (m = at(NAMED_RE, s, i)) && (slug = slugByName(m[1], repo))) {
      i += m[1].length + m[2].length; // the name stays text
      const kind = m[3].startsWith("PR") ? "pull" : "issues";
      put(i + m[3].length, `[${m[3]}](${GH}/${slug}/${kind}/${m[4]})`);
    } else if (repo && !/[\w/#&]/.test(prev) && (m = at(BARE_RE, s, i))) {
      // "PR #n" starts at the P; GitHub sends /issues/<n> of a pull request on to the pull request.
      const kind = m[1].startsWith("PR") ? "pull" : "issues";
      put(i + m[0].length, `[${m[0]}](${GH}/${repo}/${kind}/${m[2]})`);
    } else if (/[\w]/.test(s[i])) { const w = /[\w]+/y; w.lastIndex = i; i += w.exec(s)[0].length; }
    else i++;
  }
  return out + s.slice(from);
}

// The markdown with each reference as a link; code spans (any run of backticks to the same run) stay.
export function linkRefs(md, repo = null) {
  if (!md || !/#\d|github\.com\//.test(md)) return md;
  let out = "", i = 0;
  const tick = /(`+)[\s\S]*?\1/g;
  for (let m; (m = tick.exec(md));) { out += linkPlain(md.slice(i, m.index), repo) + m[0]; i = tick.lastIndex; }
  return out + linkPlain(md.slice(i), repo);
}

// "owner/repo" from a git remote URL on github.com, else null.
export function slugOf(remote) {
  const m = /github\.com[:/]([\w.-]+)\/([\w.-]+?)(?:\.git)?\/?$/.exec(String(remote ?? "").trim());
  return m ? `${m[1]}/${m[2]}` : null;
}

// The GitHub repository of a folder: read once (git, off the UI thread), then cached. null until known.
const repos = new Map();
let version = 0;
// Goes up when a new repository becomes known: the caches of linked text put it in their keys.
export const refsVersion = () => version;

// The known repository with this name: the one of the agent's own owner when two have it; else null.
function slugByName(name, repo) {
  const n = name.toLowerCase(), own = repo ? repo.split("/")[0].toLowerCase() : null;
  const hits = new Set();
  for (const s of [repo, ...repos.values()]) if (s && s.split("/")[1].toLowerCase() === n) hits.add(s);
  if (hits.size === 1) return [...hits][0];
  for (const s of hits) if (s.split("/")[0].toLowerCase() === own) return s;
  return null;
}
export function repoFor(cwd, machine = null, onKnown = null) {
  if (!cwd) return null;
  const key = `${machine ?? ""}\n${cwd}`;
  if (repos.has(key)) return repos.get(key);
  repos.set(key, null);
  exec("git", ["remote", "get-url", "origin"], { cwd, on: machine, timeout: 5000 })
    .then((r) => { const s = r.code === 0 ? slugOf(r.stdout) : null; const fresh = s && ![...repos.values()].includes(s); repos.set(key, s); if (fresh) version++; if (s) onKnown?.(); })
    .catch(() => {});
  return null;
}
export function forgetRepos() { repos.clear(); version++; }
