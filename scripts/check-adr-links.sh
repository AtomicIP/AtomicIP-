#!/usr/bin/env bash
# Fails if a relative Markdown link in docs/adr/ points at a missing file.
# Fenced code blocks and inline code are skipped so examples are not checked.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ADR_DIR="$ROOT/docs/adr"

python3 - "$ADR_DIR" <<'PY'
import os, re, sys

adr_dir = sys.argv[1]
link_re = re.compile(r'\]\(([^)\s]+)\)')
broken = []

for name in sorted(os.listdir(adr_dir)):
    if not name.endswith('.md'):
        continue
    path = os.path.join(adr_dir, name)
    in_fence = False
    with open(path, encoding='utf-8') as fh:
        for lineno, line in enumerate(fh, 1):
            if line.lstrip().startswith('```'):
                in_fence = not in_fence
                continue
            if in_fence:
                continue
            line = re.sub(r'`[^`]*`', '', line)
            for target in link_re.findall(line):
                if re.match(r'^[a-z]+:', target) or target.startswith('#'):
                    continue
                file_part = target.split('#', 1)[0]
                if not file_part:
                    continue
                if not os.path.exists(os.path.normpath(os.path.join(adr_dir, file_part))):
                    broken.append(f"{path}:{lineno}: broken link -> {target}")

if broken:
    print('\n'.join(broken))
    sys.exit(1)
print(f"ADR links OK ({adr_dir})")
PY
