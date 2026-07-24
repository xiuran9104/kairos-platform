---
name: http_probe
description: Guidance for HTTP(S) liveness, status, title, redirect, content-type, server, and header triage after URLs, hosts, or HTTP ports are discovered and before crawler, JS, fingerprinting, or validation tools are selected.
when-to-use: Use after URLs or HTTP ports are known and the next step is web surface triage.
allowed-tools:
  - http_probe
  - http_request
  - js_collect
  - url_grep
  - todo_write
---

# http_probe

Use `http_probe` for batch liveness and quick web metadata. It returns status, final URL, selected headers, content type, server, and title.

Use for:

- turning candidate URLs into confirmed live surfaces
- separating redirects, login pages, static assets, APIs, and error pages
- selecting follow-up tools such as `web_crawl`, `web_fingerprint`, `js_collect`, or `http_request`
- collecting low-risk evidence before validation

Planning notes from AboutSecurity raw material:

- Group probe results by status code, title, content length, server, technology hints, CDN/WAF hints, and redirect destination.
- Use probe metadata to choose a next step: crawl HTML pages, collect JS assets, fingerprint technology-heavy apps, and request bodies for APIs or unusual responses.
- Treat screenshots, CDN/WAF labels, and technology names as context only; they are not findings by themselves.

Recommended flow:

1. Normalize targets into explicit `http://` or `https://` URLs.
2. Run `http_probe` with `GET` unless a lightweight `HEAD` check is enough.
3. Group results by status, title, server, redirect target, and content type.
4. For interesting pages, use `http_request` for body preview or `js_collect` for script discovery.

Source reference: compiled from `wiki/skills/tool/httpx-probe/SKILL.md` into wrapper-safe triage behavior.

Do not treat a status code alone as a vulnerability. Use probe output only to choose the next read-only or bounded validation step.
