---
type: tool
title: ffuf
tags: [tool]
related: []
---

# ffuf

Purpose: Web content discovery.

## Safe Defaults

- Run only against targets listed in `scope.yaml`.
- Prefer bounded scans and clear output files.
- PentAGI's typed builder uses `ffuf -u <target>/FUZZ -w <wordlist> -of json` so Analyzer can parse structured results.
- Record command output in [[history/latest]].

## Output Signals

- Open service, status code, version string, error message, or evidence path.
- Parsed web path discoveries are reflected into [[history/latest]] and `wiki/history/findings-*.md`.
