#!/usr/bin/env python3
"""Check icode docs and metadata for stale branding from the upstream fork.

Flags references that should have been updated when claw-code became icode,
while intentionally allowing origin/lineage documentation that describes
where the project came from.
"""

import os
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
FORBIDDEN = {
    r"rusty-claude-cli": "Use 'icode-cli' instead",
    r"claw-target": "Use 'icode-target' instead",
}


def collect_files():
    """Gather all tracked docs, configs, and workflow files to scan."""
    paths = []
    # Top-level docs
    for name in ("README.md", "PARITY.md", "ROADMAP.md", "CLAUDE.md"):
        p = REPO_ROOT / name
        if p.exists():
            paths.append(p)
    # rust/Cargo.toml
    p = REPO_ROOT / "rust" / "Cargo.toml"
    if p.exists():
        paths.append(p)
    # rust/crates/*/Cargo.toml
    crates_dir = REPO_ROOT / "rust" / "crates"
    if crates_dir.exists():
        for crate in sorted(crates_dir.iterdir()):
            ct = crate / "Cargo.toml"
            if ct.exists():
                paths.append(ct)
    # .github/workflows/*.yml
    wf_dir = REPO_ROOT / ".github" / "workflows"
    if wf_dir.exists():
        for f in sorted(wf_dir.glob("*.yml")):
            paths.append(f)
    # docs/**
    docs_dir = REPO_ROOT / "docs"
    if docs_dir.exists():
        for f in sorted(docs_dir.rglob("*")):
            if f.is_file():
                paths.append(f)
    return paths


# Lines that are acceptable to reference claw-code because they describe origin.
ORIGIN_EXCEPTIONS = [
    re.compile(r"derived from", re.IGNORECASE),
    re.compile(r"forked from", re.IGNORECASE),
    re.compile(r"initial fork", re.IGNORECASE),
    re.compile(r"origin.*claw", re.IGNORECASE),
    re.compile(r"upstream.*claw", re.IGNORECASE),
    re.compile(r"leaked.*Claude Code", re.IGNORECASE),
    re.compile(r"source.*claw-code", re.IGNORECASE),
]


def is_origin_line(line):
    return any(rx.search(line) for rx in ORIGIN_EXCEPTIONS)


def scan(path):
    """Return list of (line_number, match_text, message) for stale branding."""
    issues = []
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except Exception:
        return issues
    rel = path.relative_to(REPO_ROOT)
    for i, line in enumerate(text.splitlines(), 1):
        for pattern, message in FORBIDDEN.items():
            if re.search(pattern, line, re.IGNORECASE):
                issues.append((rel, i, line.rstrip(), message))
        # Check for stale claw-code references (except origin lines)
        if "claw-code" in line and not is_origin_line(line):
            # Skip URLs to the upstream repo that are attribution links
            if "github.com/ultraworkers/claw-code" in line:
                issues.append((rel, i, line.rstrip(), "Stale upstream repo reference"))
            elif "ultraworkers/clawhip" not in line:
                issues.append(
                    (rel, i, line.rstrip(), "Use 'icode' instead of 'claw-code'")
                )
    return issues


def main():
    all_issues = []
    for path in collect_files():
        all_issues.extend(scan(path))
    if all_issues:
        print(f"Found {len(all_issues)} stale branding issue(s):\n")
        for rel, line_num, content, msg in all_issues:
            print(f"  {rel}:{line_num}: {msg}")
            print(f"    {content}")
            print()
        sys.exit(1)
    else:
        print("No stale branding found.")
        sys.exit(0)


if __name__ == "__main__":
    main()
