---
type: tool
title: nmap
tags: [tool]
related: []
---

# nmap

Purpose: Port and service enumeration.

## Safe Defaults

- Run only against targets listed in `scope.yaml`.
- Prefer bounded scans and clear output files.
- PentAGI's typed builder uses `nmap -sV -T3 --reason -oX - <host>` so Analyzer can parse XML from stdout.
- Record command output in [[history/latest]].

## Output Signals

- Open service, status code, version string, error message, or evidence path.
- Parsed open services are reflected into [[history/latest]] and `wiki/history/findings-*.md`.
