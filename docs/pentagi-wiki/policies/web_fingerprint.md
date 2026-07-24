---
name: web_fingerprint
description: Guidance for bounded web technology fingerprinting with whatweb, httpx, webanalyze, or wappalyzer-cli after HTTP liveness is confirmed and stack, CMS, framework, server, or version hints are needed for safe follow-up planning.
when-to-use: Use after HTTP liveness is confirmed and the next step is identifying web technologies.
allowed-tools:
  - command_check
  - web_fingerprint
  - http_probe
  - vuln_engine
---

# web_fingerprint

Use `web_fingerprint` for bounded web stack identification. Prefer `whatweb` first for broad compatibility; use `httpx` when JSON output and title/status/tech detection are useful.

Use for:

- identifying CMS/framework/server/version hints
- routing to WordPress, TLS, JS, or vulnerability scan workflows
- building report context and target inventory
- selecting focused, low-noise follow-up checks

Planning notes from AboutSecurity raw material:

- Combine HTTP metadata, technology signatures, service banners, and TLS certificate clues before selecting vulnerability checks.
- Prefer machine-readable output when available so technology, version, confidence, and evidence fields can be normalized.
- Service fingerprints are routing hints. Confirm exposed behavior with `http_request`, `web_vuln_scan`, `web_tls_scan`, or product-specific tools before reporting.
- For non-HTTP open ports, consider future service-fingerprint workflow rather than forcing web-only tooling.

Recommended flow:

1. Confirm the URL is in scope and live with `http_probe`.
2. Check candidate CLI availability with `command_check`.
3. Run `web_fingerprint` with `tool: "whatweb"` or `tool: "httpx"`.
4. Use technologies and versions to choose follow-up checks, not as vulnerability proof.

Source references: compiled from `wiki/skills/aboutsecurity/tool/httpx-probe/SKILL.md`, `wiki/skills/aboutsecurity/tool/fingerprintx-probe/SKILL.md`, `wiki/skills/aboutsecurity/tool/nerva-scan/SKILL.md`, and `wiki/skills/aboutsecurity/tool/gogo-scan/SKILL.md` into safe fingerprint triage guidance.

Do not report a vulnerability from fingerprint output alone. Confirm with evidence from a validation tool or manual review.
