---
name: sqlmap_scan
description: Guidance for authorization-gated, wrapper-limited sqlmap checks when a concrete in-scope URL or request has SQL injection indicators and only non-destructive proof is allowed.
when-to-use: Use only when SQL injection testing is explicitly authorized and a concrete URL/request is in scope.
allowed-tools:
  - command_check
  - sqlmap_scan
  - http_request
  - todo_write
---

# sqlmap_scan

Use `sqlmap_scan` only for authorized validation. It requires `confirm_authorized: true`; the wrapper starts with bounded defaults and records all command output/evidence.

Use for:

- confirming SQL injection signals from scanners or manual response deltas
- identifying DBMS/injection presence without dumping data
- producing tool_run evidence and normalized findings
- supporting `finding_update` after manual review

Planning notes from AboutSecurity raw material:

- Capture the exact baseline request first, including method, body, cookies, and relevant headers.
- Use POST body and Cookie inputs only when they are part of the in-scope proof request.
- Keep sqlmap as a confirmation tool after a concrete indicator exists; do not use it as broad crawling or discovery.
- Treat tamper/WAF handling as a separate reviewed workflow. The default wrapper should stay non-destructive and evidence-recorded.

Recommended flow:

1. Confirm the exact URL and parameters are in scope.
2. Use `http_request` to capture baseline behavior.
3. Use `command_check` for `sqlmap`.
4. Run `sqlmap_scan` with only the target URL and optional `data` or `cookie`.
5. Collect enough proof to confirm impact. Avoid broad dumping, OS shell, persistence, destructive file operations, or checks that affect site availability/non-test data.

Source reference: compiled from `wiki/skills/aboutsecurity/tool/sqlmap-advanced/SKILL.md`; data dumping, OS shell, file read/write, and aggressive tamper usage are intentionally excluded.

If stronger sqlmap options are needed, prefer a bounded wrapper/tool update that keeps the same non-destructive boundary and records evidence.
