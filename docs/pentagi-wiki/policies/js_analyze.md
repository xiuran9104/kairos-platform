---
name: js_analyze
description: Guidance for static JavaScript analysis that extracts API paths, URLs, sourcemaps, route hints, and suspicious key/token-like strings from text, workspace files, or fetched JS URLs.
when-to-use: Use after JavaScript content or asset URLs are available.
allowed-tools:
  - js_analyze
  - js_collect
  - url_grep
  - http_request
  - read_file
---

# js_analyze

Use `js_analyze` for JavaScript text, workspace files, or remote JavaScript URLs. URL input requires network permission.

Use for:

- extracting API routes and route templates from bundles
- identifying source map links and hidden front-end routes
- finding client-side feature flags, base URLs, and auth logic hints
- flagging token-like strings for manual review without using them

Recommended flow:

1. Analyze the asset with `js_analyze`.
2. Review `api_paths`, `urls`, `source_maps`, and `suspicious_keys`.
3. Probe API paths only within the authorized host/scope.
4. Treat suspicious keys as leads; validate safely without using or exfiltrating third-party secrets.

Do not execute JavaScript or generated payloads from assets. This skill is for static extraction and triage.
