#!/usr/bin/env node
/**
 * office — the deterministic core of the-office.
 *
 * Every command in this file is pure computation: no model is involved, no
 * network call is made. Verification records the code and contract it observed.
 * Anything requiring judgement belongs to an agent, not here.
 *
 * Zero dependencies by design — Claude Code guarantees Node, and nothing else.
 */

import fs from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const VERSION = '0.4.0';

const STATUSES = ['pending', 'in-progress', 'review', 'blocked', 'completed'];
const TIERS = ['fast', 'standard', 'deep'];
const REQUIRED = ['id', 'task_no', 'status', 'tier', 'checks', 'dod'];

/* ------------------------------------------------------------------ *
 * YAML subset — enough for frontmatter and config, nothing more.
 * ------------------------------------------------------------------ */

const indentOf = (l) => l.match(/^ */)[0].length;

function stripComment(s) {
  let q = null;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (q) { if (c === q) q = null; continue; }
    if (c === '"' || c === "'") { q = c; continue; }
    if (c === '#' && (i === 0 || /\s/.test(s[i - 1]))) return s.slice(0, i);
  }
  return s;
}

function scalar(s) {
  s = s.trim();
  if (s === '') return '';
  if ((s.startsWith('"') && s.endsWith('"')) || (s.startsWith("'") && s.endsWith("'"))) {
    return s.slice(1, -1);
  }
  if (s === 'null' || s === '~') return null;
  if (s === 'true') return true;
  if (s === 'false') return false;
  if (/^-?\d+$/.test(s)) return parseInt(s, 10);
  if (/^-?\d*\.\d+$/.test(s)) return parseFloat(s);
  return s;
}

function splitInline(s) {
  const out = [];
  let buf = '', q = null;
  for (const c of s) {
    if (q) { buf += c; if (c === q) q = null; continue; }
    if (c === '"' || c === "'") { q = c; buf += c; continue; }
    if (c === ',') { out.push(buf); buf = ''; continue; }
    buf += c;
  }
  if (buf.trim()) out.push(buf);
  return out;
}

function parseList(lines, i, indent) {
  const out = [];
  while (i < lines.length) {
    const raw = lines[i];
    if (!raw.trim() || /^\s*#/.test(raw)) { i++; continue; }
    if (indentOf(raw) < indent) break;
    const m = /^\s*-\s*(.*)$/.exec(raw);
    if (!m) break;
    out.push(scalar(stripComment(m[1])));
    i++;
  }
  return [out, i];
}

function parseMap(lines, i, indent) {
  const out = {};
  while (i < lines.length) {
    const raw = lines[i];
    if (!raw.trim() || /^\s*#/.test(raw)) { i++; continue; }
    const ind = indentOf(raw);
    if (ind < indent) break;
    if (ind > indent) { i++; continue; }

    const m = /^\s*([A-Za-z_][\w.-]*):\s*(.*)$/.exec(raw);
    if (!m) { i++; continue; }
    const key = m[1];
    if (Object.hasOwn(out, key)) throw new Error(`duplicate YAML key "${key}"`);
    const rest = stripComment(m[2]).trim();

    if (rest === '|' || rest === '|-' || rest === '>' || rest === '>-') {
      const buf = [];
      const bi = indent + 2;
      i++;
      while (i < lines.length && (lines[i].trim() === '' || indentOf(lines[i]) >= bi)) {
        buf.push(lines[i].slice(bi));
        i++;
      }
      while (buf.length && buf[buf.length - 1].trim() === '') buf.pop();
      out[key] = buf.join('\n');
      continue;
    }

    if (rest === '') {
      let j = i + 1;
      while (j < lines.length && (!lines[j].trim() || /^\s*#/.test(lines[j]))) j++;
      if (j < lines.length && indentOf(lines[j]) > indent) {
        if (/^\s*-/.test(lines[j])) {
          const [list, ni] = parseList(lines, j, indentOf(lines[j]));
          out[key] = list; i = ni; continue;
        }
        const [map, ni] = parseMap(lines, j, indentOf(lines[j]));
        out[key] = map; i = ni; continue;
      }
      out[key] = [];
      i++; continue;
    }

    if (rest.startsWith('[')) {
      const inner = rest.slice(1, rest.lastIndexOf(']'));
      out[key] = inner.trim() === '' ? [] : splitInline(inner).map(scalar);
      i++; continue;
    }

    out[key] = scalar(rest);
    i++;
  }
  return [out, i];
}

const parseYaml = (lines) => parseMap(lines, 0, 0)[0];

function splitFrontmatter(text) {
  const lines = text.split('\n');
  if (lines[0]?.trim() !== '---') return null;
  for (let i = 1; i < lines.length; i++) {
    if (lines[i].trim() === '---') {
      return { fmLines: lines.slice(1, i), bodyLines: lines.slice(i + 1), endIndex: i };
    }
  }
  return null;
}

/**
 * Surgical single-key rewrite. Deliberately not a full re-serialize — a
 * round-trip through a hand-rolled emitter would reformat the human-authored
 * parts of the file, and the board is meant to stay readable in a diff.
 */
function setScalar(text, key, value) {
  const lines = text.split('\n');
  const fm = splitFrontmatter(text);
  if (!fm) throw new Error('no frontmatter to update');
  const re = new RegExp(`^${key}:`);
  const rendered = `${key}: ${emit(value)}`;
  for (let i = 1; i < fm.endIndex; i++) {
    if (re.test(lines[i])) { lines[i] = rendered; return lines.join('\n'); }
  }
  lines.splice(fm.endIndex, 0, rendered);
  return lines.join('\n');
}

function emit(v) {
  if (v === null || v === undefined) return 'null';
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  const s = String(v);
  if (s === '') return '""';
  if (/[:#[\]{}"'\n]|^\s|\s$/.test(s)) return JSON.stringify(s);
  return s;
}

/* ------------------------------------------------------------------ *
 * Repo + board access
 * ------------------------------------------------------------------ */

function findRoot(from = process.cwd()) {
  let d = path.resolve(from);
  for (;;) {
    if (fs.existsSync(path.join(d, '.the-office'))) return d;
    const up = path.dirname(d);
    if (up === d) return null;
    d = up;
  }
}

function requireRoot() {
  const root = findRoot();
  if (!root) die('no .the-office/ found in this directory or any parent.\nRun `office init` first, or `/office-onboard` to bootstrap the harness.');
  return root;
}

const DEFAULT_CONFIG = {
  tiers: { fast: 'haiku', standard: 'sonnet', deep: 'opus' },
  caps: { plan_iterations: 3, review_iterations: 3 },
  gates: { after_po_clarification: true, after_plan_converges: true, before_harness_change: true },
  janitor: { recurrence_threshold: 3 },
  stacks: [],
};

function loadConfig(root) {
  const p = path.join(root, '.the-office', 'config.yml');
  if (!fs.existsSync(p)) return { ...DEFAULT_CONFIG };
  const parsed = parseYaml(fs.readFileSync(p, 'utf8').split('\n'));
  return {
    ...DEFAULT_CONFIG, ...parsed,
    tiers: { ...DEFAULT_CONFIG.tiers, ...(parsed.tiers || {}) },
    caps: { ...DEFAULT_CONFIG.caps, ...(parsed.caps || {}) },
    gates: { ...DEFAULT_CONFIG.gates, ...(parsed.gates || {}) },
    janitor: { ...DEFAULT_CONFIG.janitor, ...(parsed.janitor || {}) },
  };
}

function loadTasks(root, feature) {
  const base = path.join(root, '.the-office', 'features');
  if (!fs.existsSync(base)) return [];
  const out = [];
  for (const f of fs.readdirSync(base).sort()) {
    if (feature && f !== feature) continue;
    const dir = path.join(base, f);
    if (!fs.statSync(dir).isDirectory()) continue;
    for (const file of fs.readdirSync(dir).sort()) {
      if (!/^task-.*\.md$/.test(file)) continue;
      const p = path.join(dir, file);
      const text = fs.readFileSync(p, 'utf8');
      const fm = splitFrontmatter(text);
      const expectedId = `${f}/${file.replace(/\.md$/, '')}`;
      if (!fm) { out.push({ file: p, feature: f, expectedId, data: {}, text, broken: 'missing frontmatter' }); continue; }
      out.push({ file: p, feature: f, expectedId, data: parseYaml(fm.fmLines), text });
    }
  }
  return out;
}

const findTask = (tasks, id) =>
  tasks.find((t) => t.data.id === id) ||
  tasks.find((t) => t.expectedId === id) ||
  tasks.find((t) => t.expectedId.endsWith(`/${id}`));

function writeTask(t, text) {
  fs.writeFileSync(t.file, text);
  t.text = text;
}

/* ------------------------------------------------------------------ *
 * Output helpers
 * ------------------------------------------------------------------ */

const TTY = process.stdout.isTTY && !process.env.NO_COLOR;
const c = {
  dim: (s) => (TTY ? `\x1b[2m${s}\x1b[0m` : s),
  bold: (s) => (TTY ? `\x1b[1m${s}\x1b[0m` : s),
  red: (s) => (TTY ? `\x1b[31m${s}\x1b[0m` : s),
  green: (s) => (TTY ? `\x1b[32m${s}\x1b[0m` : s),
  yellow: (s) => (TTY ? `\x1b[33m${s}\x1b[0m` : s),
  cyan: (s) => (TTY ? `\x1b[36m${s}\x1b[0m` : s),
};

function die(msg, code = 1) {
  console.error(`${c.red('office:')} ${msg}`);
  process.exit(code);
}

function table(headers, rows) {
  const all = [headers, ...rows];
  const w = headers.map((_, i) => Math.max(...all.map((r) => String(r[i] ?? '').length)));
  const line = (r, f = (s) => s) => f(r.map((cell, i) => String(cell ?? '').padEnd(w[i])).join('  ').trimEnd());
  console.log(line(headers, c.dim));
  console.log(c.dim(w.map((n) => '-'.repeat(n)).join('  ')));
  for (const r of rows) console.log(line(r));
}

const STATUS_MARK = {
  pending: c.dim('·'), 'in-progress': c.cyan('>'), review: c.yellow('?'),
  blocked: c.red('!'), completed: c.green('x'),
};

/* ------------------------------------------------------------------ *
 * Glob matching (for scope allowlists)
 * ------------------------------------------------------------------ */

function globToRe(g) {
  let re = '';
  for (let i = 0; i < g.length; i++) {
    const ch = g[i];
    if (ch === '*') {
      if (g[i + 1] === '*') {
        if (g[i + 2] === '/') { re += '(?:.*/)?'; i += 2; } else { re += '.*'; i += 1; }
      } else re += '[^/]*';
    } else if (ch === '?') re += '[^/]';
    else if ('.+^${}()|[]\\'.includes(ch)) re += `\\${ch}`;
    else re += ch;
  }
  return new RegExp(`^${re}$`);
}

const matchesAny = (file, globs) => globs.some((g) => globToRe(g).test(file));

/* ------------------------------------------------------------------ *
 * Git helpers
 * ------------------------------------------------------------------ */

function git(root, args) {
  const r = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
  return r.status === 0 ? (args.includes('-z') ? r.stdout : r.stdout.trim()) : null;
}

// Only board bookkeeping is excluded. Installed agents and harness controls are
// code: changes to them must remain visible to scope and verification.
const isBoardState = (f) => f.startsWith('.the-office/features/') || f === '.the-office/findings.jsonl' || f === '.the-office/execution.lock';
const digest = (value) => createHash('sha256').update(value).digest('hex');
function gitRequired(root, args) {
  const result = git(root, args);
  if (result === null) die(`git ${args.join(' ')} failed; verification requires a Git repository with a commit.`);
  return result;
}
function gitNames(root, args) {
  return gitRequired(root, args).split('\0').filter(Boolean);
}
function changedFiles(root, base = 'HEAD') {
  return [...new Set([
    ...gitNames(root, ['diff', '--no-renames', '--name-only', '-z', base, '--']),
    ...gitNames(root, ['diff', '--no-renames', '--name-only', '-z', '--cached', '--']),
    ...gitNames(root, ['ls-files', '--others', '--exclude-standard', '-z']),
  ])].filter((f) => !isBoardState(f)).sort();
}
function taskContext(id) {
  const root = requireRoot();
  const tasks = loadTasks(root);
  const t = findTask(tasks, id);
  if (!t) die(`no task matching "${id}".`);
  return { root, tasks, t };
}
function requireStatus(t, allowed) {
  if (!allowed.includes(t.data.status)) die(`task ${t.data.id} is "${t.data.status}"; expected one of: ${allowed.join(', ')}.`);
}
function requireBudget(t) {
  if (!Number.isInteger(t.data.attempts ?? 0) || (t.data.attempts ?? 0) < 0 || !Number.isInteger(t.data.max_attempts ?? 3) || (t.data.max_attempts ?? 3) < 1) die('attempts and max_attempts must be valid non-negative/positive integers.');
}
function requireDependencies(t, tasks) {
  if (!Array.isArray(t.data.depends_on ?? [])) die('depends_on must be a list.');
  for (const dep of t.data.depends_on ?? []) {
    if (!tasks.some((other) => other.data.id === dep && other.data.status === 'completed')) die(`dependency ${dep} is not completed.`);
  }
}
function requireIdle(tasks, t) {
  const active = tasks.find((other) => other.file !== t?.file && ['in-progress', 'review'].includes(other.data.status));
  if (active) die(`work is in flight: ${active.data.id}. Sequential execution: finish it first.`);
}
function requireClean(root) {
  if (changedFiles(root).length) die('commit task code before claiming, review, or completion; uncommitted code cannot be tied to a commit. Board bookkeeping may remain dirty.');
}
function requireBase(root, t) {
  if (!t.data.base_commit) die('task has no base_commit; claim a pending task first. For legacy active tasks, record the actual pre-task commit in base_commit, then rerun check and scope.');
  const base = String(t.data.base_commit);
  if (!/^[0-9a-f]{40,64}$/.test(base)) die('base_commit must be a full commit SHA.');
  gitRequired(root, ['merge-base', '--is-ancestor', base, 'HEAD']);
  return base;
}
function evidencePath(root, t) {
  const dir = path.resolve(root, gitRequired(root, ['rev-parse', '--git-path', 'office-verification']));
  fs.mkdirSync(dir, { recursive: true });
  return path.join(dir, `${digest(t.expectedId)}.json`);
}
const SPEC_TYPES = ['bug', 'feature', 'refactor'];
const SPEC_SECTIONS = {
  bug: ['Current behavior', 'Expected behavior', 'Reproduction', 'Environment and evidence', 'Hypotheses'],
  feature: ['User flows', 'Business rules', 'Data and integrations', 'Failure and empty states'],
  refactor: ['Structural problem', 'Change boundary', 'Behavior invariants', 'Migration strategy'],
};
function featureDir(root, slug) {
  if (!/^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(slug ?? '')) throw new Error('feature slug must use lowercase letters, digits, and hyphens');
  return path.join(root, '.the-office', 'features', slug);
}
const meaningful = (v) => typeof v === 'string' && v.trim() && !/\bTODO\b|\{\{/.test(v);
const isMap = (v) => v && typeof v === 'object' && !Array.isArray(v);
function readSpec(root, slug) {
  const dir = featureDir(root, slug);
  const file = path.join(dir, 'overview.md');
  const text = fs.existsSync(file) ? fs.readFileSync(file, 'utf8').replace(/\r\n/g, '\n') : '';
  const fm = splitFrontmatter(text);
  const data = fm ? parseYaml(fm.fmLines) : {};
  const required = fs.existsSync(path.join(dir, 'spec-approval.json')) || loadTasks(root, slug).some((t) => t.data.spec_required);
  if (!Object.hasOwn(data, 'spec_version')) {
    if (required) throw new Error(`${slug}: required spec is missing or its schema marker was removed`);
    return null; // Explicit compatibility for pre-spec boards.
  }
  return { dir, text, data, hash: digest(text) };
}
function validateSpec(root, slug, plan = false) {
  const spec = readSpec(root, slug);
  if (!spec) throw new Error(`${slug}: legacy overview; migrate to a typed spec first`);
  const { data, text } = spec;
  const fail = (message) => { throw new Error(`${slug}: ${message}`); };
  if (data.spec_version !== 1 || data.id !== slug || !SPEC_TYPES.includes(data.type)) fail('invalid spec_version, id, or type');
  if (!isMap(data.requirements) || !Object.keys(data.requirements).length) fail('requirements must be a non-empty map');
  if (!isMap(data.acceptance_criteria) || !Object.keys(data.acceptance_criteria).length) fail('acceptance_criteria must be a non-empty map');
  if (!Array.isArray(data.open_questions)) fail('open_questions must be a list');
  for (const [id, description] of Object.entries(data.requirements)) {
    if (!/^REQ-[0-9]{3,}$/.test(id) || !meaningful(description)) fail(`invalid requirement ${id}`);
    if (!Object.values(data.acceptance_criteria).some((ac) => ac?.requirement === id)) fail(`${id} has no acceptance criterion`);
  }
  for (const [id, ac] of Object.entries(data.acceptance_criteria)) {
    if (!/^AC-[0-9]{3,}$/.test(id) || !isMap(ac) || !Object.hasOwn(data.requirements, ac.requirement) || !meaningful(ac.description)) fail(`invalid acceptance criterion ${id} or unknown requirement`);
  }
  const sections = new Map();
  for (const part of text.split(/^## /m).slice(1)) {
    const end = part.indexOf('\n');
    const name = part.slice(0, end).trim();
    if (sections.has(name)) fail(`duplicate section ${name}`);
    sections.set(name, part.slice(end + 1).replace(/<!--[\s\S]*?-->/g, '').trim());
  }
  for (const name of ['Request', 'Objective', 'Out of scope', 'Constraints', 'Assumptions', 'Harness impact', ...SPEC_SECTIONS[data.type]]) {
    if (!meaningful(sections.get(name))) fail(`fill section ${name}`);
  }
  const tasks = loadTasks(root, slug);
  if (plan) {
    if (!tasks.length) fail('plan has no tasks');
    const covered = new Set();
    for (const t of tasks) {
      const refs = t.data.requirements;
      const criteria = t.data.acceptance_criteria;
      if (!Array.isArray(refs) || !refs.length || new Set(refs).size !== refs.length || refs.some((id) => !Object.hasOwn(data.requirements, id))) fail(`${t.expectedId}: invalid requirements references`);
      if (!Array.isArray(criteria) || !criteria.length || new Set(criteria).size !== criteria.length || criteria.some((id) => !Object.hasOwn(data.acceptance_criteria, id))) fail(`${t.expectedId}: invalid acceptance_criteria references`);
      for (const id of criteria) {
        if (!refs.includes(data.acceptance_criteria[id].requirement)) fail(`${t.expectedId}: ${id} is not linked to a declared requirement`);
        covered.add(id);
      }
      if (refs.some((id) => !criteria.some((ac) => data.acceptance_criteria[ac].requirement === id))) fail(`${t.expectedId}: requirement reference has no criterion`);
      if (!['regression', 'acceptance', 'preservation'].includes(t.data.verification_mode)) fail(`${t.expectedId}: invalid verification_mode`);
    }
    for (const ac of Object.keys(data.acceptance_criteria)) if (!covered.has(ac)) fail(`${ac} has no task coverage`);
  }
  return spec;
}
function specApproval(spec) {
  try { return JSON.parse(fs.readFileSync(path.join(spec.dir, 'spec-approval.json'), 'utf8')); }
  catch (err) { if (err.code === 'ENOENT' || err instanceof SyntaxError) return null; throw err; }
}
function requireSpec(root, t) {
  if (!readSpec(root, t.feature)) return null;
  const spec = validateSpec(root, t.feature, true);
  if (spec.data.open_questions.length) throw new Error(`${t.feature}: resolve open_questions before execution`);
  if (specApproval(spec)?.hash !== spec.hash) throw new Error(`${t.feature}: spec approval is missing or stale; confirm the updated spec with the human`);
  return spec.hash;
}

function snapshot(root, t) {
  const base = requireBase(root, t);
  if (path.resolve(gitRequired(root, ['rev-parse', '--show-toplevel'])) !== path.resolve(root)) die('the board must be at the Git repository root.');
  const head = gitRequired(root, ['rev-parse', 'HEAD']);
  const files = [...new Set([
    ...gitNames(root, ['ls-files', '-z']),
    ...gitNames(root, ['ls-files', '--others', '--exclude-standard', '-z']),
  ])].filter((f) => !isBoardState(f)).sort();
  const hashes = files.map((f) => {
    const full = path.join(root, f);
    try {
      const stat = fs.lstatSync(full);
      if (stat.isDirectory()) die(`cannot attest directory/submodule ${f}; use a repository without submodules for this workflow.`);
      return [f, stat.mode, digest(stat.isSymbolicLink() ? fs.readlinkSync(full) : fs.readFileSync(full))];
    } catch (err) { if (err.code === 'ENOENT') return [f, null]; throw err; }
  });
  const contract = Object.fromEntries(['id', 'title', 'tier', 'depends_on', 'scope', 'checks', 'dod', 'max_attempts', 'requirements', 'acceptance_criteria', 'verification_mode'].map((key) => [key, t.data[key]]));
  return { base, head, spec: requireSpec(root, t), contract: digest(JSON.stringify(contract)), tree: digest(JSON.stringify(hashes)) };
}
function readEvidence(root, t) {
  try { return JSON.parse(fs.readFileSync(evidencePath(root, t), 'utf8')); }
  catch (err) { if (err.code === 'ENOENT' || err instanceof SyntaxError) return {}; throw err; }
}
function saveEvidence(root, t, evidence) {
  const file = evidencePath(root, t);
  fs.writeFileSync(`${file}.tmp`, JSON.stringify(evidence, null, 2) + '\n');
  fs.renameSync(`${file}.tmp`, file);
}
const sameSnapshot = (a, b) => JSON.stringify(a) === JSON.stringify(b);
function requireVerified(root, t) {
  requireClean(root);
  const current = snapshot(root, t);
  const evidence = readEvidence(root, t);
  for (const kind of ['check', 'scope']) {
    if (!sameSnapshot(evidence[kind]?.snapshot, current)) die(`missing or stale ${kind} evidence; rerun office check and office scope for the current code and contract.`);
  }
}
function changeStatus(t, status, fields = {}, note) {
  let text = setScalar(t.text, 'status', status);
  for (const [key, value] of Object.entries(fields)) text = setScalar(text, key, value);
  if (note) text = text.replace(/\s*$/, `\n- ${note}\n`);
  // One rename publishes status and associated fields together.
  fs.writeFileSync(`${t.file}.tmp`, text);
  fs.renameSync(`${t.file}.tmp`, t.file);
  t.text = text;
  console.log(`${status} ${t.data.id}`);
}
function reasonArg(argv) {
  const i = argv.indexOf('--reason');
  const reason = i >= 0 ? argv[i + 1]?.trim() : '';
  if (!reason) die('requires --reason "...".');
  return reason;
}

/* ------------------------------------------------------------------ *
 * Commands
 * ------------------------------------------------------------------ */

const cmds = {};

cmds.init = (argv) => {
  const root = findRoot() ?? process.cwd();
  const dir = path.join(root, '.the-office');
  if (fs.existsSync(dir) && !argv.includes('--force')) {
    // Say where it found the board. Resolving upward is correct — nested boards
    // would be worse — but silently targeting an ancestor is confusing enough
    // that it cost a CI debugging cycle.
    const here = path.resolve(root) === path.resolve(process.cwd());
    die(here
      ? `.the-office/ already exists here. Pass --force to overwrite config.`
      : `.the-office/ already exists in a parent directory: ${root}\n`
        + `You are in ${process.cwd()}.\n`
        + `office resolves the board by walking up, so this repo already has one. `
        + `cd there to work on it, or --force to overwrite that config.`);
  }
  fs.mkdirSync(path.join(dir, 'features'), { recursive: true });
  const cfg = path.join(dir, 'config.yml');
  if (!fs.existsSync(cfg) || argv.includes('--force')) {
    fs.writeFileSync(cfg, `version: ${VERSION}

# Model tiers. Task files reference the tier, never a model id, so a model
# release does not invalidate every task on the board.
tiers:
  fast: haiku
  standard: sonnet
  deep: opus

# Loop bounds. Exceeding one escalates to a human rather than spinning.
caps:
  plan_iterations: 3
  review_iterations: 3

# Where a human must approve. before_harness_change is the one worth defending:
# a new hook changes every contributor's workflow.
gates:
  after_po_clarification: true
  after_plan_converges: true
  before_harness_change: true

janitor:
  recurrence_threshold: 3

# Populated by \`office audit\`.
stacks: []
`);
  }
  const findings = path.join(dir, 'findings.jsonl');
  if (!fs.existsSync(findings)) fs.writeFileSync(findings, '');
  console.log(`${c.green('initialised')} ${path.relative(process.cwd(), dir) || '.the-office'}/`);
  console.log(c.dim('next: run /office-onboard to audit the repo and propose a harness.'));
};

cmds.board = (argv) => {
  const root = requireRoot();
  const feature = argv.find((a) => !a.startsWith('-'));
  const tasks = loadTasks(root, feature);
  if (!tasks.length) { console.log(c.dim('no tasks on the board.')); return; }

  const byFeature = new Map();
  for (const t of tasks) {
    if (!byFeature.has(t.feature)) byFeature.set(t.feature, []);
    byFeature.get(t.feature).push(t);
  }
  for (const [f, list] of byFeature) {
    console.log(`\n${c.bold(f)}`);
    table(
      ['', 'TASK', 'STATUS', 'TIER', 'DEPENDS ON', 'TITLE'],
      list.map((t) => [
        STATUS_MARK[t.data.status] ?? c.red('?'),
        t.data.id ?? t.expectedId,
        t.data.status ?? c.red('MISSING'),
        t.data.tier ?? '-',
        (t.data.depends_on ?? []).join(', ') || '-',
        t.data.title ?? '',
      ]),
    );
  }
  const done = tasks.filter((t) => t.data.status === 'completed').length;
  const blocked = tasks.filter((t) => t.data.status === 'blocked').length;
  console.log(`\n${c.dim(`${done}/${tasks.length} completed`)}${blocked ? c.red(`  ${blocked} blocked`) : ''}`);
};

cmds.next = () => {
  const root = requireRoot();
  const tasks = loadTasks(root);
  const byId = new Map(tasks.map((t) => [t.data.id ?? t.expectedId, t]));
  requireIdle(tasks);
  const ready = tasks
    .filter((t) => t.data.status === 'pending')
    .filter((t) => (t.data.depends_on ?? []).every((d) => byId.get(d)?.data.status === 'completed'))
    .sort((a, b) => (a.data.task_no ?? 0) - (b.data.task_no ?? 0));

  if (!ready.length) {
    const blocked = tasks.filter((t) => t.data.status === 'blocked');
    if (blocked.length) {
      console.error(c.red('no ready task — these are blocked and need a human:'));
      for (const t of blocked) console.error(`  ${t.data.id}`);
    } else if (tasks.some((t) => ['in-progress', 'review'].includes(t.data.status))) {
      console.error(c.yellow('no ready task — work is in flight. Sequential execution: finish it first.'));
    } else {
      console.error(c.dim('no ready task — board is complete or empty.'));
    }
    process.exit(1);
  }
  console.log(ready[0].data.id ?? ready[0].expectedId);
};

cmds.claim = (argv) => {
  const { root, tasks, t } = taskContext(argv[0] ?? die('usage: office claim <task-id>'));
  requireStatus(t, ['pending', 'blocked']);
  requireSpec(root, t);
  requireIdle(tasks, t);
  requireDependencies(t, tasks);
  requireBudget(t);
  const n = (t.data.attempts ?? 0) + 1;
  if (n > (t.data.max_attempts ?? 3)) die('max_attempts reached; a human must resolve the blocker and explicitly increase the budget before retrying.');
  requireClean(root);
  if (path.resolve(gitRequired(root, ['rev-parse', '--show-toplevel'])) !== path.resolve(root)) die('the board must be at the Git repository root.');
  const base = t.data.base_commit ? requireBase(root, t) : gitRequired(root, ['rev-parse', 'HEAD']);
  saveEvidence(root, t, {});
  changeStatus(t, 'in-progress', { attempts: n, base_commit: base });
};

cmds.retry = (argv) => {
  const reason = reasonArg(argv);
  const { root, tasks, t } = taskContext(argv[0]);
  requireStatus(t, ['review']);
  requireBudget(t);
  requireIdle(tasks, t);
  requireDependencies(t, tasks);
  saveEvidence(root, t, {});
  if ((t.data.attempts ?? 0) >= (t.data.max_attempts ?? 3)) {
    changeStatus(t, 'blocked', {}, `**blocked:** retry budget exhausted. ${reason}`);
    die('max_attempts reached; escalated to a human.');
  }
  changeStatus(t, 'in-progress', { attempts: (t.data.attempts ?? 0) + 1 }, `**review findings:** ${reason}`);
};

cmds.review = (argv) => {
  const { root, tasks, t } = taskContext(argv[0] ?? die('usage: office review <task-id>'));
  requireStatus(t, ['in-progress']);
  requireIdle(tasks, t);
  requireDependencies(t, tasks);
  requireVerified(root, t);
  changeStatus(t, 'review');
};

cmds.done = (argv) => {
  const { root, tasks, t } = taskContext(argv[0] ?? die('usage: office done <task-id>'));
  requireStatus(t, ['review']);
  requireIdle(tasks, t);
  requireDependencies(t, tasks);
  requireVerified(root, t);
  changeStatus(t, 'completed', {
    branch: gitRequired(root, ['rev-parse', '--abbrev-ref', 'HEAD']),
    commit: gitRequired(root, ['rev-parse', 'HEAD']),
  });
};

cmds.block = (argv) => {
  const reason = reasonArg(argv);
  const { root, t } = taskContext(argv[0]);
  requireStatus(t, ['pending', 'in-progress', 'review', 'blocked']);
  saveEvidence(root, t, {});
  changeStatus(t, 'blocked', {}, `**blocked:** ${reason}`);
};

cmds.check = (argv) => {
  const { root, t } = taskContext(argv[0] ?? die('usage: office check <task-id>'));
  const checks = t.data.checks ?? [];
  if (!Array.isArray(checks) || !checks.length || checks.some((cmd) => typeof cmd !== 'string' || !cmd.trim())) die('task has no valid checks. The definition of done is not executable.');
  requireStatus(t, ['in-progress', 'review']);
  const evidence = readEvidence(root, t);
  delete evidence.check;
  saveEvidence(root, t, evidence);
  const before = snapshot(root, t);
  const results = [];
  for (const cmd of checks) {
    console.log(cmd);
    const r = spawnSync(cmd, { cwd: root, shell: true, stdio: 'inherit' });
    if (r.status !== 0) die(`FAIL ${cmd} (exit ${r.status}); definition of done not met.`);
    results.push({ command: cmd, exit: r.status });
  }
  const after = snapshot(root, taskContext(t.data.id).t);
  if (!sameSnapshot(before, after)) die('checks changed code or contract; review those changes and rerun.');
  evidence.check = { snapshot: after, results, at: new Date().toISOString() };
  saveEvidence(root, t, evidence);
  console.log(`all checks pass for ${t.data.id}`);
};

cmds.scope = (argv) => {
  const { root, t } = taskContext(argv[0] ?? die('usage: office scope <task-id>'));
  requireStatus(t, ['in-progress', 'review']);
  const evidence = readEvidence(root, t);
  delete evidence.scope;
  saveEvidence(root, t, evidence);
  const globs = t.data.scope;
  if (!Array.isArray(globs) || !globs.length || globs.some((g) => typeof g !== 'string' || !g.trim())) die('task must declare a non-empty scope allowlist.');
  const current = snapshot(root, t);
  const files = changedFiles(root, current.base);
  const outside = files.filter((f) => !matchesAny(f, globs));
  if (outside.length) die(`files outside the declared scope of ${t.data.id}:\n${outside.join('\n')}`);
  if (!sameSnapshot(current, snapshot(root, taskContext(t.data.id).t))) die('code or contract changed during scope verification; rerun.');
  evidence.scope = { snapshot: current, files, at: new Date().toISOString() };
  saveEvidence(root, t, evidence);
  console.log(`in scope — ${files.length} changed file(s) all match ${t.data.id}`);
};

cmds.validate = () => {
  const root = requireRoot();
  const tasks = loadTasks(root);
  const errors = [];
  const warnings = [];
  const seen = new Map();

  for (const t of tasks) {
    const rel = path.relative(root, t.file);
    const at = (m) => errors.push(`${rel}: ${m}`);

    if (t.broken) { at(t.broken); continue; }
    for (const k of REQUIRED) {
      if (t.data[k] === undefined || t.data[k] === null || t.data[k] === '') at(`missing required key "${k}"`);
    }
    if (t.data.id && t.data.id !== t.expectedId) at(`id "${t.data.id}" does not match its path (expected "${t.expectedId}")`);
    if (t.data.status && !STATUSES.includes(t.data.status)) at(`status "${t.data.status}" not one of: ${STATUSES.join(', ')}`);
    if (t.data.tier && !TIERS.includes(t.data.tier)) at(`tier "${t.data.tier}" not one of: ${TIERS.join(', ')}`);
    if (t.data.model) warnings.push(`${rel}: has a "model" key — use "tier" instead; model ids rot.`);

    const checks = t.data.checks;
    if (Array.isArray(checks) && checks.length === 0) at('checks is empty — the definition of done is not executable');
    else if (checks !== undefined && !Array.isArray(checks)) at('checks must be a list of shell commands');

    const id = t.data.id ?? t.expectedId;
    if (seen.has(id)) at(`duplicate id "${id}" (also in ${path.relative(root, seen.get(id).file)})`);
    else seen.set(id, t);

    if (t.data.max_attempts !== undefined && (!Number.isInteger(t.data.max_attempts) || t.data.max_attempts < 1)) at('max_attempts must be an integer >= 1');
    if (t.data.attempts !== undefined && (!Number.isInteger(t.data.attempts) || t.data.attempts < 0)) at('attempts must be an integer >= 0');
    if (!Array.isArray(t.data.depends_on ?? [])) at('depends_on must be a list');
    if (t.data.status === 'completed' && !t.data.commit) {
      warnings.push(`${rel}: completed but no commit recorded — use \`office done\` rather than editing by hand.`);
    }
  }

  for (const t of tasks) {
    for (const d of t.data.depends_on ?? []) {
      if (!seen.has(d)) errors.push(`${path.relative(root, t.file)}: depends_on "${d}" does not exist`);
    }
  }

  // Cycle detection over the dependency graph.
  const state = new Map();
  const stack = [];
  const visit = (id) => {
    if (state.get(id) === 'done') return;
    if (state.get(id) === 'open') {
      const at = stack.indexOf(id);
      errors.push(`dependency cycle: ${[...stack.slice(at), id].join(' -> ')}`);
      return;
    }
    state.set(id, 'open');
    stack.push(id);
    for (const d of seen.get(id)?.data.depends_on ?? []) if (seen.has(d)) visit(d);
    stack.pop();
    state.set(id, 'done');
  };
  for (const id of seen.keys()) visit(id);

  const features = path.join(root, '.the-office', 'features');
  if (fs.existsSync(features)) for (const slug of fs.readdirSync(features)) {
    if (!fs.statSync(path.join(features, slug)).isDirectory()) continue;
    try { if (readSpec(root, slug)) validateSpec(root, slug, tasks.some((t) => t.feature === slug)); }
    catch (err) { errors.push(err.message); }
  }

  for (const w of warnings) console.error(`${c.yellow('warn')}  ${w}`);
  if (errors.length) {
    for (const e of errors) console.error(`${c.red('error')} ${e}`);
    console.error(`\n${c.red(`${errors.length} error(s)`)} across ${tasks.length} task(s).`);
    process.exit(1);
  }
  console.log(c.green(`ok — ${tasks.length} task(s) valid${warnings.length ? `, ${warnings.length} warning(s)` : ''}`));
};

/* ------------------------------------------------------------------ *
 * audit — the computational half of harness assessment
 * ------------------------------------------------------------------ */

const STACK_MARKERS = {
  python: ['pyproject.toml', 'setup.py', 'setup.cfg', 'requirements.txt'],
  typescript: ['package.json'],
  go: ['go.mod'],
  rust: ['Cargo.toml'],
};

const SENSOR_PROBES = {
  python: [
    { id: 'formatter', label: 'ruff format / black', files: ['ruff.toml', '.ruff.toml'], inFile: ['pyproject.toml', /\[tool\.(ruff|black)\]/] },
    { id: 'linter', label: 'ruff / flake8', inFile: ['pyproject.toml', /\[tool\.(ruff|flake8)\]/], files: ['.flake8', 'ruff.toml'] },
    { id: 'types', label: 'mypy / pyright', files: ['mypy.ini', 'pyrightconfig.json'], inFile: ['pyproject.toml', /\[tool\.(mypy|pyright)\]/] },
    { id: 'tests', label: 'pytest', files: ['pytest.ini', 'tox.ini'], inFile: ['pyproject.toml', /\[tool\.pytest/], dirs: ['tests', 'test'] },
    { id: 'coverage', label: 'coverage threshold', files: ['.coveragerc'], inFile: ['pyproject.toml', /\[tool\.coverage/] },
  ],
  typescript: [
    { id: 'formatter', label: 'prettier / biome', files: ['.prettierrc', '.prettierrc.json', 'prettier.config.js', 'biome.json'] },
    { id: 'linter', label: 'eslint / biome', files: ['eslint.config.js', 'eslint.config.mjs', '.eslintrc', '.eslintrc.json', '.eslintrc.cjs', 'biome.json'] },
    { id: 'types', label: 'typescript', files: ['tsconfig.json'] },
    { id: 'tests', label: 'vitest / jest', files: ['vitest.config.ts', 'vitest.config.js', 'jest.config.js', 'jest.config.ts'], inFile: ['package.json', /"(vitest|jest)"/] },
    { id: 'coverage', label: 'coverage threshold', inFile: ['vitest.config.ts', /thresholds?/] },
  ],
  go: [
    { id: 'linter', label: 'golangci-lint', files: ['.golangci.yml', '.golangci.yaml', '.golangci.toml'] },
    { id: 'types', label: 'go build (inherent)', always: true },
    { id: 'tests', label: 'go test', glob: /_test\.go$/ },
    { id: 'formatter', label: 'gofmt (inherent)', always: true },
  ],
  rust: [
    { id: 'linter', label: 'clippy', files: ['clippy.toml', '.clippy.toml'], inFile: ['Cargo.toml', /\[lints/] },
    { id: 'types', label: 'rustc (inherent)', always: true },
    { id: 'formatter', label: 'rustfmt', files: ['rustfmt.toml', '.rustfmt.toml'], always: true },
    { id: 'tests', label: 'cargo test', dirs: ['tests'], glob: /#\[test\]/ },
  ],
};

const UNIVERSAL_PROBES = [
  // Stack-specific test probes miss a repo whose suite is a shell script or a
  // Makefile target. Found by onboarding the-office with its own audit.
  { id: 'tests', label: 'test entrypoint', files: ['tests/run.sh', 'test.sh', 'Makefile', 'justfile', 'Taskfile.yml'], inFile: ['package.json', /"scripts"[\s\S]*"test"/] },
  { id: 'pre-commit-hook', label: 'pre-commit / lefthook / husky', files: ['.pre-commit-config.yaml', 'lefthook.yml', '.husky', '.githooks'] },
  { id: 'ci', label: 'CI pipeline', dirs: ['.github/workflows', '.gitlab-ci.yml', '.circleci'] },
  { id: 'guides', label: 'CLAUDE.md / AGENTS.md', files: ['CLAUDE.md', 'AGENTS.md'] },
  { id: 'editor-config', label: 'editorconfig', files: ['.editorconfig'] },
];

function probe(root, p) {
  const has = (rel) => fs.existsSync(path.join(root, rel));
  if (p.files?.some(has)) return true;
  if (p.dirs?.some(has)) return true;
  if (p.inFile) {
    const [file, re] = p.inFile;
    if (has(file)) { try { if (re.test(fs.readFileSync(path.join(root, file), 'utf8'))) return true; } catch { /* unreadable */ } }
  }
  if (p.glob) {
    const hit = walk(root, 4).some((f) => p.glob.test(f));
    if (hit) return true;
  }
  return !!p.always;
}

function walk(root, maxDepth) {
  const skip = new Set(['node_modules', '.git', 'target', 'dist', 'build', '.venv', 'venv', '__pycache__', 'vendor', '.the-office']);
  const out = [];
  const rec = (dir, depth) => {
    if (depth > maxDepth) return;
    let entries;
    try { entries = fs.readdirSync(dir, { withFileTypes: true }); } catch { return; }
    for (const e of entries) {
      if (skip.has(e.name)) continue;
      const full = path.join(dir, e.name);
      if (e.isDirectory()) rec(full, depth + 1);
      else out.push(path.relative(root, full));
    }
  };
  rec(root, 0);
  return out;
}

function scoreHarnessability(root, stacks, sensors) {
  const has = (rel) => fs.existsSync(path.join(root, rel));
  const present = new Set(sensors.filter((s) => s.present).map((s) => s.id));

  // Typing strength — how much a compiler can prove before a test runs.
  let typing = 0;
  if (stacks.includes('rust') || stacks.includes('go')) typing = 25;
  else if (stacks.includes('typescript')) {
    typing = has('tsconfig.json') ? 15 : 5;
    try {
      if (/"strict"\s*:\s*true/.test(fs.readFileSync(path.join(root, 'tsconfig.json'), 'utf8'))) typing = 25;
    } catch { /* no tsconfig */ }
  } else if (stacks.includes('python')) typing = present.has('types') ? 18 : 6;

  // Boundary clarity — proxied by top-level source organisation.
  const files = walk(root, 3);
  const topDirs = new Set(files.map((f) => f.split('/')[0]).filter((d) => !d.includes('.')));
  const boundaries = Math.min(20, 6 + Math.min(topDirs.size, 7) * 2);

  const tests = present.has('tests') ? 25 : 0;   // any probe with id 'tests', stack or universal
  const build = (has('package-lock.json') || has('pnpm-lock.yaml') || has('yarn.lock') ||
                 has('poetry.lock') || has('uv.lock') || has('Cargo.lock') || has('go.sum')) ? 15 : 4;
  const controls = Math.round((present.size / Math.max(sensors.length, 1)) * 15);

  const total = typing + boundaries + tests + build + controls;
  return {
    total,
    band: total >= 75 ? 'high' : total >= 45 ? 'medium' : 'low',
    components: { typing, boundaries, tests, build, controls },
    max: { typing: 25, boundaries: 20, tests: 25, build: 15, controls: 15 },
  };
}

cmds.audit = (argv) => {
  const root = findRoot() ?? process.cwd();
  const stacks = Object.entries(STACK_MARKERS)
    .filter(([, markers]) => markers.some((m) => fs.existsSync(path.join(root, m))))
    .map(([s]) => s);

  // package.json without tsconfig is JS, not TS — still the typescript pack.
  const sensors = [];
  for (const s of stacks) {
    for (const p of SENSOR_PROBES[s] ?? []) {
      sensors.push({ stack: s, id: p.id, label: p.label, present: probe(root, p) });
    }
  }
  for (const p of UNIVERSAL_PROBES) {
    sensors.push({ stack: 'universal', id: p.id, label: p.label, present: probe(root, p) });
  }

  const score = scoreHarnessability(root, stacks, sensors);
  const isGreenfield = (git(root, ['rev-list', '--count', 'HEAD']) ?? '0') === '0' || walk(root, 3).length < 20;

  const report = {
    version: VERSION,
    root,
    class: isGreenfield ? 'greenfield' : 'legacy',
    stacks,
    harnessability: score,
    sensors,
    missing: sensors.filter((s) => !s.present).map((s) => `${s.stack}:${s.id}`),
  };

  if (argv.includes('--json')) { console.log(JSON.stringify(report, null, 2)); return; }

  console.log(`\n${c.bold('harness audit')}  ${c.dim(root)}`);
  console.log(`${c.dim('class')}    ${report.class}`);
  console.log(`${c.dim('stacks')}   ${stacks.join(', ') || c.yellow('none detected')}`);
  console.log(`${c.dim('score')}    ${score.total}/100 (${score.band})`);
  console.log(c.dim(`         typing ${score.components.typing}/25  boundaries ${score.components.boundaries}/20  tests ${score.components.tests}/25  build ${score.components.build}/15  controls ${score.components.controls}/15`));
  console.log('');
  table(['', 'STACK', 'CONTROL', 'DETECTED AS'],
    sensors.map((s) => [s.present ? c.green('x') : c.red('·'), s.stack, s.id, s.label]));
  console.log(`\n${c.dim('This is the computational half only. Boundary and convention findings need the Office Manager.')}`);
};

/* ------------------------------------------------------------------ *
 * packs — the installable control catalogue
 * ------------------------------------------------------------------ */

const HERE = path.dirname(fileURLToPath(import.meta.url));

/** Installed layout is .claude/office/{bin,packs}; the dev repo is payload/bin + packs/. */
function packsDir() {
  for (const rel of ['../packs', '../../packs']) {
    const d = path.resolve(HERE, rel);
    if (fs.existsSync(d)) return d;
  }
  return null;
}

function loadPack(stack) {
  const dir = packsDir();
  if (!dir) die('no packs directory found next to office.mjs.');
  const p = path.join(dir, stack, 'pack.json');
  if (!fs.existsSync(p)) die(`no pack for stack "${stack}". Available: ${fs.readdirSync(dir).join(', ')}`);
  return { ...JSON.parse(fs.readFileSync(p, 'utf8')), dir: path.join(dir, stack) };
}

cmds.pack = (argv) => {
  const dir = packsDir();
  const sub = argv[0];

  if (!sub || sub === 'list') {
    const stacks = fs.readdirSync(dir).filter((d) => fs.existsSync(path.join(dir, d, 'pack.json')));
    table(['STACK', 'CONTROLS', 'DETECTED BY'], stacks.map((s) => {
      const p = loadPack(s);
      return [s, p.controls.length, (p.detect ?? []).join(', ')];
    }));
    return;
  }

  if (sub === 'show') {
    const stack = argv[1] ?? die('usage: office pack show <stack> [--json]');
    const pack = loadPack(stack);
    if (argv.includes('--json')) { console.log(JSON.stringify(pack, null, 2)); return; }

    // Strangler ordering: cheap high-signal controls first. This ordering is
    // the difference between a legacy harness that survives and one that gets
    // switched off by the first person who hits a wall of errors.
    const ordered = [...pack.controls].sort((a, b) => (a.legacy_order ?? 99) - (b.legacy_order ?? 99));
    console.log(`\n${c.bold(stack)} pack ${c.dim(pack.dir)}`);
    console.log(c.dim('install in this order on a legacy repo:\n'));
    table(['#', 'CONTROL', 'CELL', 'COST', 'CHECK'],
      ordered.map((k, i) => [i, k.id, k.cell.replace('computational-', 'comp-').replace('inferential-', 'inf-'), k.cost, k.check ?? '-']));
    console.log('');
    for (const k of ordered) {
      if (!k.note) continue;
      console.log(`${c.cyan(k.id)} ${c.dim(`— ${(k.files ?? []).join(', ') || 'no files'}`)}`);
      console.log(`  ${k.note}\n`);
    }
    return;
  }

  if (sub === 'files') {
    const stack = argv[1] ?? die('usage: office pack files <stack> [control]');
    const control = argv[2];
    const pack = loadPack(stack);
    for (const k of pack.controls) {
      if (control && k.id !== control) continue;
      for (const f of k.files ?? []) console.log(path.join(pack.dir, f));
    }
    return;
  }

  die(`unknown subcommand "${sub}". Try: list, show, files.`);
};

/* ------------------------------------------------------------------ *
 * findings — the ledger the steering loop reads
 * ------------------------------------------------------------------ */

function ledgerPath(root) { return path.join(root, '.the-office', 'findings.jsonl'); }

function readLedger(root) {
  const p = ledgerPath(root);
  if (!fs.existsSync(p)) return [];
  return fs.readFileSync(p, 'utf8').split('\n').filter(Boolean).map((l) => {
    try { return JSON.parse(l); } catch { return null; }
  }).filter(Boolean);
}

cmds.findings = (argv) => {
  const root = requireRoot();
  const sub = argv[0];

  if (sub === 'add') {
    const flag = (n) => { const i = argv.indexOf(`--${n}`); return i >= 0 ? argv[i + 1] : null; };
    const klass = flag('class');
    if (!klass) die('usage: office findings add --class <slug> --task <id> [--lens <maintainability|architecture|behaviour>] [--note "..."]\n\nThe class is what makes recurrence detectable — reuse an existing slug when the defect is the same kind.');
    const entry = {
      class: klass,
      task: flag('task'),
      lens: flag('lens') ?? 'maintainability',
      severity: flag('severity') ?? 'normal',
      note: flag('note') ?? '',
      seq: readLedger(root).length + 1,
    };
    fs.appendFileSync(ledgerPath(root), `${JSON.stringify(entry)}\n`);
    console.log(`${c.green('logged')} ${klass} ${c.dim(`(#${entry.seq})`)}`);
    const count = readLedger(root).filter((f) => f.class === klass).length;
    const threshold = loadConfig(root).janitor.recurrence_threshold;
    if (count >= threshold) {
      console.log(c.yellow(`\n"${klass}" has now recurred ${count}x (threshold ${threshold}).`));
      console.log(c.yellow('Hand this to the Janitor: a computational control should be catching it, not a reviewer.'));
    }
    return;
  }

  if (sub === 'recur') {
    const threshold = loadConfig(root).janitor.recurrence_threshold;
    const counts = new Map();
    for (const f of readLedger(root)) counts.set(f.class, (counts.get(f.class) ?? 0) + 1);
    const over = [...counts.entries()].filter(([, n]) => n >= threshold).sort((a, b) => b[1] - a[1]);
    if (!over.length) { console.log(c.dim(`no finding class has reached the recurrence threshold (${threshold}).`)); process.exit(1); }
    table(['COUNT', 'CLASS', 'LENSES'], over.map(([k, n]) => [
      n, k, [...new Set(readLedger(root).filter((f) => f.class === k).map((f) => f.lens))].join(', '),
    ]));
    console.log(c.dim('\nEach of these should become a computational control.'));
    return;
  }

  if (sub === 'list' || !sub) {
    const all = readLedger(root);
    if (!all.length) { console.log(c.dim('ledger is empty.')); return; }
    table(['#', 'CLASS', 'LENS', 'TASK', 'NOTE'],
      all.map((f) => [f.seq, f.class, f.lens, f.task ?? '-', (f.note ?? '').slice(0, 56)]));
    return;
  }
  die(`unknown subcommand "${sub}". Try: add, recur, list.`);
};

/* ------------------------------------------------------------------ *
 * scaffolding
 * ------------------------------------------------------------------ */

cmds.spec = (argv) => {
  const root = requireRoot();
  const [action, slug] = argv;
  if (!['validate', 'status', 'approve'].includes(action)) die('usage: office spec validate|status|approve <slug> [--plan] [--hash SHA --by NAME]');
  const spec = validateSpec(root, slug, argv.includes('--plan'));
  if (action === 'approve') {
    const flag = (key) => { const i = argv.indexOf(`--${key}`); return i < 0 ? null : argv[i + 1]; };
    if (spec.data.open_questions.length) die('resolve open_questions before approval; record resolved decisions or accepted assumptions in the spec');
    if (flag('hash') !== spec.hash || !flag('by')?.trim()) die('approval requires the exact current --hash and a non-empty --by after human confirmation');
    const file = path.join(spec.dir, 'spec-approval.json');
    fs.writeFileSync(`${file}.tmp`, JSON.stringify({ hash: spec.hash, by: flag('by'), at: new Date().toISOString() }, null, 2) + '\n');
    fs.renameSync(`${file}.tmp`, file);
  }
  console.log(JSON.stringify({ id: slug, type: spec.data.type, hash: spec.hash, status: specApproval(spec)?.hash === spec.hash ? 'approved' : specApproval(spec) ? 'stale' : 'draft', open_questions: spec.data.open_questions.length }));
};

cmds.feature = (argv) => {
  const root = requireRoot();
  if (argv[0] !== 'new') die('usage: office feature new <slug> [--type bug|feature|refactor] [--title "..."]');
  const slug = argv[1];
  const flag = (key, fallback) => { const i = argv.indexOf(`--${key}`); return i < 0 ? fallback : argv[i + 1]; };
  const type = flag('type', 'feature');
  if (!SPEC_TYPES.includes(type)) die('type must be bug, feature, or refactor');
  const dir = featureDir(root, slug);
  if (fs.existsSync(dir)) die(`feature "${slug}" already exists.`);
  const templates = ['../templates', '../../templates'].map((rel) => path.resolve(HERE, rel));
  const template = templates.map((d) => path.join(d, `spec-${type}.md.tmpl`)).find((f) => fs.existsSync(f));
  if (!template) die('spec templates missing; upgrade the installed payload');
  const text = fs.readFileSync(template, 'utf8').replaceAll('{{SLUG}}', slug).replaceAll('{{TITLE}}', flag('title', slug));
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'overview.md'), text);
  console.log(`created .the-office/features/${slug}/overview.md (${type}); fill the draft and run office spec validate ${slug}`);
};

cmds.task = (argv) => {
  const root = requireRoot();
  if (argv[0] !== 'new') die('usage: office task new <feature> --title "..." [--tier standard]');
  const feature = argv[1] ?? die('usage: office task new <feature> --title "..."');
  const flag = (n, d) => { const i = argv.indexOf(`--${n}`); return i >= 0 ? argv[i + 1] : d; };
  const dir = featureDir(root, feature);
  if (!fs.existsSync(dir)) die(`no feature "${feature}". Run: office feature new ${feature}`);

  const existing = fs.readdirSync(dir).filter((f) => /^task-\d+\.md$/.test(f));
  const no = Math.max(0, ...existing.map((f) => Number(f.match(/^task-(\d+)\.md$/)[1]))) + 1;
  const name = `task-${String(no).padStart(2, '0')}`;
  const title = flag('title', 'untitled');
  const spec = readSpec(root, feature);
  const mode = spec ? { bug: 'regression', feature: 'acceptance', refactor: 'preservation' }[spec.data.type] : null;

  fs.writeFileSync(path.join(dir, `${name}.md`), `---
id: ${feature}/${name}
task_no: ${no}
title: ${emit(title)}
${spec ? `spec_required: true\nrequirements: []\nacceptance_criteria: []\nverification_mode: ${mode}\n` : ''}depends_on: []
status: pending
tier: ${flag('tier', 'standard')}
scope: []
checks: []
sensors_added: []
dod: |
  TODO — replace with the observable behaviour that proves this task is done.
attempts: 0
max_attempts: ${flag('max-attempts', '3')}
base_commit: null
branch: null
commit: null
---

## Context

## Approach

## Notes
`);
  console.log(`${c.green('created')} .the-office/features/${feature}/${name}.md`);
  console.log(c.yellow('checks is empty — office validate will reject this until the DoD is executable.'));
};

cmds.version = () => console.log(VERSION);
cmds.capabilities = () => console.log(JSON.stringify({ lifecycle: 1, verification: 1 }));

cmds.help = () => {
  console.log(`${c.bold('office')} ${c.dim(VERSION)} — deterministic core for the-office

${c.dim('board')}
  init [--force]              scaffold .the-office/ in this repo
  board [feature]             render the kanban
  next                        print the next ready task id (exit 1 if none)
  validate                    schema, duplicate ids, orphan deps, cycles

${c.dim('lifecycle')}
  claim <id>                  pending|blocked -> in-progress, increments attempts
  retry <id> --reason "..."   review -> in-progress, or blocked at the attempt limit
  review <id>                 verified in-progress -> review
  done <id>                   -> completed, records branch + commit from git
  block <id> --reason "..."   -> blocked, escalates to a human

${c.dim('verification')}
  check <id>                  run the task's checks; exit on first failure
  scope <id>                  assert base-to-working-tree diff stays inside scope globs

${c.dim('harness')}
  audit [--json]              detect stacks, existing controls, harnessability
  pack list                   available sensor packs
  pack show <stack>           controls in strangler order, with install notes
  pack files <stack> [ctrl]   absolute paths of the files a control installs
  findings add --class <slug> --task <id> [--lens L] [--note "..."]
  findings recur              classes at or over the recurrence threshold
  findings list               the whole ledger

${c.dim('scaffolding')}
  spec validate|status <slug> [--plan]
  spec approve <slug> --hash SHA --by NAME
  feature new <slug> [--type bug|feature|refactor] [--title "..."]
  task new <feature> --title "..." [--tier fast|standard|deep]

${c.dim('Nothing here uses a model. If a decision needs judgement, it belongs to an agent.')}`);
};

/* ------------------------------------------------------------------ */

const [, , cmd = 'help', ...rest] = process.argv;
const handler = cmds[cmd] ?? (['--version', '-v'].includes(cmd) ? cmds.version : null)
  ?? (['--help', '-h'].includes(cmd) ? cmds.help : null);
if (!handler) die(`unknown command "${cmd}". Run \`office help\`.`);
try {
  // Serialize state/evidence writes across CLI processes, including long checks.
  // A killed process leaves an explicit lock; never silently steal a live run.
  if (['claim', 'retry', 'review', 'done', 'block', 'check', 'scope', 'feature', 'task'].includes(cmd) || (cmd === 'spec' && rest[0] === 'approve')) {
    const root = requireRoot();
    const lock = path.join(root, '.the-office', 'execution.lock');
    let fd;
    try { fd = fs.openSync(lock, 'wx'); }
    catch (err) {
      if (err.code === 'EEXIST') die('another command owns .the-office/execution.lock. If its process has exited, remove the stale lock and retry.');
      throw err;
    }
    fs.writeFileSync(fd, String(process.pid));
    fs.closeSync(fd);
    process.on('exit', () => fs.rmSync(lock, { force: true }));
  }
  handler(rest);
} catch (err) {
  die(err.message);
}
