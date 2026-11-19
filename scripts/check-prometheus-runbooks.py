#!/usr/bin/env python3
"""Check that every Prometheus alert links to a runbook that exists (#1267).

Each alert rule in monitoring/prometheus/*.yml must set
`annotations.runbook_url` to a repo-relative `path[#anchor]`. The file must
exist, and the anchor, if given, must match a heading (GitHub slug rules) or an
explicit `<a id="...">` in that file. If the legacy `annotations.runbook` key is
also present it must equal `runbook_url`, so the two cannot drift apart.

Usage: check-prometheus-runbooks.py [rules_dir]
Exits 1 and lists every problem when any check fails.
"""
import glob
import os
import re
import sys

import yaml

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
DEFAULT_RULES_DIR = os.path.join(REPO, "monitoring", "prometheus")


def github_slug(heading):
    text = heading.strip().lower()
    text = re.sub(r"[^\w\- ]", "", text)
    return text.replace(" ", "-")


def anchors_in(path):
    anchors, seen, in_fence = set(), {}, False
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            if line.lstrip().startswith("```"):
                in_fence = not in_fence
                continue
            if in_fence:
                continue
            match = re.match(r"^#{1,6}\s+(.*?)\s*#*\s*$", line)
            if match:
                slug = github_slug(match.group(1))
                count = seen.get(slug, 0)
                seen[slug] = count + 1
                anchors.add(slug if count == 0 else f"{slug}-{count}")
            for explicit in re.finditer(r'<a\s+(?:id|name)="([^"]+)"', line):
                anchors.add(explicit.group(1))
    return anchors


def check_rules_dir(rules_dir):
    errors = []
    for rules_file in sorted(glob.glob(os.path.join(rules_dir, "*.yml"))):
        rel = os.path.relpath(rules_file, REPO)
        with open(rules_file, encoding="utf-8") as fh:
            doc = yaml.safe_load(fh) or {}
        for group in doc.get("groups") or []:
            for rule in group.get("rules") or []:
                if "alert" not in rule:
                    continue
                where = f"{rel}: alert {rule['alert']}"
                annotations = rule.get("annotations") or {}
                url = str(annotations.get("runbook_url") or "").strip()
                if not url:
                    errors.append(f"{where}: missing annotations.runbook_url")
                    continue
                if "runbook" in annotations and str(annotations["runbook"]).strip() != url:
                    errors.append(f"{where}: annotations.runbook does not match runbook_url")
                path, _, anchor = url.partition("#")
                target = os.path.join(REPO, path)
                if not os.path.isfile(target):
                    errors.append(f"{where}: runbook_url file not found: {path}")
                    continue
                if anchor and anchor not in anchors_in(target):
                    errors.append(f"{where}: anchor '#{anchor}' not found in {path}")
    return errors


def main(argv):
    rules_dir = os.path.abspath(argv[1]) if len(argv) > 1 else DEFAULT_RULES_DIR
    errors = check_rules_dir(rules_dir)
    if errors:
        print("Prometheus runbook check failed:")
        for error in errors:
            print(f"  - {error}")
        return 1
    print(f"Prometheus runbook check passed for {rules_dir}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
