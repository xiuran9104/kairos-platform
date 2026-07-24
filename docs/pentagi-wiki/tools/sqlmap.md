---
type: tool
title: sqlmap
tags: [tool]
related: []
---

# sqlmap

Purpose: SQL injection validation for authorized targets.

## Safe Defaults

- Run only against targets listed in `scope.yaml`.
- Prefer bounded scans and clear output files.
- PentAGI's typed builder uses `sqlmap -u <url> --batch --smart` and treats tool-reported injectable markers as vulnerability signals requiring manual validation.
- Record command output in [[history/latest]].

## Output Signals

- Open service, status code, version string, error message, or evidence path.
- Parsed SQL injection markers are reflected into [[history/latest]] and `wiki/history/findings-*.md`.
