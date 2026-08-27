#!/usr/bin/env python3
"""Architectural fitness functions for the omv workspace. Exit 1 on any violation.

Each check here was a paragraph in CLAUDE.md first. A rule that a machine can
check should not rely on a human remembering to check it, so the prose now
points at this file rather than carrying the rule alone.

Run it directly:  .github/scripts/arch-check.py
"""
import json, re, subprocess, sys, pathlib

ROOT = pathlib.Path(subprocess.run(["git","rev-parse","--show-toplevel"],
        capture_output=True,text=True,check=True).stdout.strip())
fail = []

# --- F1: dependency direction ------------------------------------------------
# CLAUDE.md: "omv -> everything; omv-config/omv-syntax -> omv-core;
#             omv-core -> nothing in-workspace"
ALLOWED = {
    "omv":        {"omv-core","omv-config","omv-syntax","omv-lsp","omv-find"},
    "omv-config": {"omv-core"},
    "omv-syntax": {"omv-core"},
    "omv-core":   set(),
    "omv-lsp":    set(),
    "omv-find":   set(),
}
meta = json.loads(subprocess.run(["cargo","metadata","--format-version","1","--no-deps"],
        cwd=ROOT,capture_output=True,text=True,check=True).stdout)
ws = {p["name"] for p in meta["packages"]}
for p in meta["packages"]:
    direct = {d["name"] for d in p["dependencies"]} & ws
    extra = direct - ALLOWED.get(p["name"], set())
    for e in sorted(extra):
        fail.append(f"F1 dep-direction: {p['name']} must not depend on {e}")

# --- F2: async quarantine ----------------------------------------------------
# CLAUDE.md: "No other crate depends on tokio, and none should."
for p in meta["packages"]:
    direct = {d["name"] for d in p["dependencies"]}
    if "tokio" in direct and p["name"] != "omv-lsp":
        fail.append(f"F2 async-quarantine: {p['name']} declares a tokio dependency")

# --- F3: omv-core purity -----------------------------------------------------
# CLAUDE.md: "Never reach into the terminal, filesystem-beyond-the-buffer, or a
# language server from omv-core." buffer.rs load/save is the sanctioned exception.
FORBIDDEN = ["std::process","std::net","std::io::stdout","std::io::stdin",
             "std::env","tokio","crossterm","ratatui"]
FS_EXEMPT = {"buffer.rs"}
def strip_comments(t):
    # Line comments only: the codebase uses no /* */ blocks. Comments legitimately
    # NAME the forbidden crates ("stays free of ... tokio"), so matching raw text
    # produces false positives.
    return "\n".join(re.sub(r"//.*$", "", ln) for ln in t.splitlines())

for f in sorted((ROOT/"crates/omv-core/src").rglob("*.rs")):
    text = strip_comments(f.read_text())
    for pat in FORBIDDEN:
        if pat in text:
            fail.append(f"F3 core-purity: {f.relative_to(ROOT)} references {pat}")
    if "std::fs" in text and f.name not in FS_EXEMPT:
        fail.append(f"F3 core-purity: {f.relative_to(ROOT)} touches std::fs "
                    f"(only {'/'.join(FS_EXEMPT)} may)")

# --- F4: keymap assets byte-identical ---------------------------------------
# CLAUDE.md: "these two files are currently byte-identical and must stay in sync."
a = (ROOT/"crates/omv-config/assets/default.yaml").read_bytes()
b = (ROOT/"examples/config.yaml").read_bytes()
if a != b:
    fail.append("F4 keymap-sync: crates/omv-config/assets/default.yaml and "
                "examples/config.yaml have diverged")

# --- F5: every action is reachable ------------------------------------------
src = (ROOT/"crates/omv-core/src/action.rs").read_text()
pairs = re.findall(r'(\w+)\s*=>\s*"([a-z0-9_]+)"', src.split("actions! {",1)[1])
yaml_words = set(re.findall(r'[a-z0-9_]+', (ROOT/"crates/omv-config/assets/default.yaml").read_text()))
# Only the `:command` table counts as an alternative entry point, not every
# `Action::X` in the file -- the dispatch match arm mentions every action by
# construction, so matching the whole file would make F5 unable to fail.
ed_src = (ROOT/"crates/omv-core/src/editor.rs").read_text()
cmd_region = ed_src.split("match cmd {",1)[1] if "match cmd {" in ed_src else ""
dispatched = set(re.findall(r'Action::(\w+)', cmd_region))
for variant, name in pairs:
    if name not in yaml_words and variant not in dispatched:
        fail.append(f"F5 unreachable-action: {name} is bound to no key and "
                    f"dispatched by no command")

for line in fail: print(line, file=sys.stderr)
print(f"arch-check: {len(pairs)} actions, {len(meta['packages'])} crates, "
      f"{len(fail)} violation(s)")
sys.exit(1 if fail else 0)
