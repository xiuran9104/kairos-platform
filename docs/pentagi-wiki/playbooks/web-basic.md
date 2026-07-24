---
type: playbook
title: Web Basic Authorized Testing
tags: [web, enumeration]
related: [policies/nmap_scan, policies/dir_fuzz, tools/http_request]
---

# Web Basic Authorized Testing

1. Confirm the target is in `scope.yaml`.
2. Enumerate exposed ports and service versions with [[policies/nmap_scan]].
3. Probe HTTP responses with [[tools/http_request]].
4. Discover content with [[policies/dir_fuzz]] when a web origin is confirmed.
5. Validate findings manually before exploitation. Keep evidence in [[history/latest]].
