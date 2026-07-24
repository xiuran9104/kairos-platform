---
name: web_crawl
description: Guidance for bounded web crawling with hakrawler, gospider, or optional katana when a live in-scope web app needs URL, form, script, and route discovery before probing, JS analysis, or vulnerability hypothesis generation.
when-to-use: Use when a live web app needs URL discovery before probing or analysis.
allowed-tools:
  - command_check
  - web_crawl
  - url_grep
  - js_collect
  - http_probe
---

# web_crawl

Use `web_crawl` for bounded URL discovery. The default crawler is `hakrawler`; use `gospider` when installed and JSON-ish crawler output is useful; use `katana` only when the operator has installed it and explicitly requests it.

Use for:

- collecting same-site URLs before `http_probe`
- discovering forms, links, JS assets, and shallow API paths
- feeding routes into `vuln_hypothesis_plan`
- building a small crawl corpus for `url_grep` and `js_collect`

Planning notes from AboutSecurity raw material:

- Use standard crawling first; reserve headless crawling for SPA pages where static crawling misses routes.
- JS endpoint extraction is valuable, but endpoints remain candidates until probed.
- Filter static assets separately from API paths, forms, and parameterized URLs.
- Keep crawl depth and result count bounded; expand from high-signal areas such as authenticated test pages only after scope is clear.

Inputs:

- `url`: an explicit `http://` or `https://` URL that is in scope
- `tool`: `hakrawler`, `gospider`, or optional `katana`
- `depth`: start at `1` or `2`
- `limit`: cap candidate volume before follow-up probing

Recommended flow:

1. Start from a confirmed live URL.
2. Check crawler availability with `command_check`.
3. Use `tool: "hakrawler"` unless there is a reason to choose another installed crawler.
4. Use `depth: 1` or `depth: 2` initially.
5. Extract and deduplicate URLs with `url_grep` if needed.
6. Probe discovered URLs with `http_probe`.
7. Send interesting JS assets to `js_collect` or `js_analyze`.

Output handling:

- Treat crawler output as candidates, not verified endpoints.
- Group URLs by host, path prefix, extension, and parameter names.
- Keep raw tool output as evidence when it affects scope or validation decisions.

Source reference: compiled from `wiki/skills/aboutsecurity/tool/katana-crawl/SKILL.md` with traffic-heavy and unbounded crawl modes excluded.

Do not use this skill for recursive large crawls, authenticated destructive workflows, or traffic-heavy scans.
