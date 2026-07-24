---
name: dir_fuzz
description: Guidance for bounded directory, file, and content discovery with ffuf, feroxbuster, or dirsearch wrappers after a live in-scope web base URL is confirmed and path discovery is authorized.
when-to-use: Use after a web host is confirmed live and path discovery is authorized.
allowed-tools:
  - command_check
  - dir_fuzz
  - http_probe
  - todo_write
---

# dir_fuzz

Use `dir_fuzz` for rate-limited directory/content discovery. It wraps `ffuf`, `feroxbuster`, or `dirsearch` with bounded parameters.

Use for:

- discovering hidden static paths, admin panels, uploads, backups, and API prefixes
- validating route candidates from JS or archive discovery
- producing evidence-backed candidate paths for `http_probe`

Planning notes from AboutSecurity raw material:

- Pick the smallest relevant wordlist first; expand only when early results justify it.
- Use response status, content length, redirects, and stable error bodies to filter noise.
- Keep recursion shallow and deliberate. Deep recursion should follow confirmed directory hits and a clear reason.
- Record interesting hits with method, status, length, title/content-type when available, and follow-up tool.

Recommended flow:

1. Confirm authorization and live base URL.
2. Use `command_check` for `ffuf`, `feroxbuster`, or `dirsearch`.
3. Choose a small wordlist first and set `rate` conservatively.
4. Probe interesting hits with `http_probe` or `http_request`.

Source references: compiled from `wiki/skills/tool/ffuf-fuzz/SKILL.md` and `wiki/skills/tool/dirsearch-fuzz/SKILL.md` with high-rate and credential brute-force usage excluded.

Avoid credential brute force, parameter brute force against state-changing endpoints, recursive large scans, and high-rate settings.
