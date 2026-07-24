---
name: web_vuln_scan
description: Guidance for authorization-gated, evidence-recorded web vulnerability scanning with nuclei or nikto when scope is explicit and scanner findings must be normalized into PentAGI findings.
when-to-use: Use only when vulnerability scanning is explicitly authorized for the target URL.
allowed-tools:
  - command_check
  - web_vuln_scan
  - vuln_engine
  - todo_write
---

# web_vuln_scan

Use `web_vuln_scan` for bounded scanner execution. It requires `confirm_authorized: true`.

Use for:

- known vulnerability and misconfiguration checks
- template-based triage after fingerprinting
- producing normalized findings and evidence refs
- feeding `vuln_engine`, `validation_dispatch`, and reporting

Planning notes from AboutSecurity raw material:

- Prefer focused scanner configuration: known product, known template, known severity, or a bounded default profile.
- Use fingerprint and crawl output to reduce scanner noise before running vulnerability checks.
- For nuclei, severity and template selection are triage controls; they do not replace manual validation.
- For nikto, treat server misconfiguration and dangerous file hits as leads that need request/response proof.
- Keep scanner output normalized through `vuln_engine` and attach `finding_set` evidence before validation or reporting.

Recommended flow:

1. Confirm URL scope and authorization.
2. Check `nuclei` or `nikto` availability with `command_check`.
3. Prefer `nuclei` with conservative `rate` and optional `severity`.
4. Review returned `findings` and `evidence_refs`; the tool automatically writes a `finding_set` evidence record when findings are present.
5. Use `vuln_engine` only if additional normalization from saved stdout is needed.

Source references: compiled from `wiki/skills/aboutsecurity/tool/nuclei-scan/SKILL.md` and `wiki/skills/aboutsecurity/tool/nikto-scan/SKILL.md`; exploitation templates, high-risk checks, and data-dumping behavior are excluded.

Do not use custom payload loops, high-rate scans, WAF bypass templates, exploitation templates, or data-dumping checks.
