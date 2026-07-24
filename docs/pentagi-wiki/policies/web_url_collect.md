---
name: web_url_collect
description: Guidance for optional passive historical URL collection with gau or waybackurls when they are already installed and an authorized domain needs archived route, parameter, or endpoint discovery before probing.
when-to-use: Use during Web recon when a domain is authorized and historical URLs may reveal routes or parameters.
allowed-tools:
  - command_check
  - web_url_collect
  - url_grep
  - http_probe
---

# web_url_collect

Use `web_url_collect` for passive URL collection from public archives. This wrapper is optional: `gau` and `waybackurls` are not required by doctor, so always check availability before planning around it.

Use for:

- discovering old paths, parameters, and API routes
- supplementing live crawl output
- finding candidate JS, backup, export, upload, and callback endpoints
- feeding candidates into `http_probe` and `vuln_hypothesis_plan`

Planning notes from AboutSecurity raw material:

- Archive sources are useful for parameters, legacy API prefixes, old JS files, and renamed endpoints.
- Filter static assets into a separate bucket; prioritize parameterized URLs, API-like paths, upload/export/download/callback routes, and JavaScript.
- Preserve source and collection timestamp when an archived URL materially affects a hypothesis or finding.

Inputs:

- `domain`: a bare in-scope domain such as `example.com`
- `tool`: required; choose `gau` or `waybackurls` only after `command_check`
- `timeout_ms`: keep collection bounded

Recommended flow:

1. Confirm the domain is in scope.
2. Use `command_check` for `gau` or `waybackurls`.
3. Run `web_url_collect` only when one is available.
4. Deduplicate and group by path, extension, and parameter names.
5. Probe only in-scope URLs with `http_probe`.
6. Send parameterized routes to `vuln_hypothesis_plan`.

Fallback:

- If neither `gau` nor `waybackurls` is installed, skip this skill and use `web_crawl`, `url_grep`, `js_collect`, and `js_analyze` from live in-scope pages.

Source reference: compiled from `wiki/skills/aboutsecurity/tool/gau-urlfetch/SKILL.md` into passive, bounded URL collection guidance.

Do not treat archived URLs as currently live or in scope without follow-up confirmation.
