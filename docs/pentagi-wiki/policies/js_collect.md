---
name: js_collect
description: Guidance for discovering JavaScript assets from a live page and optionally downloading bounded previews when routes, APIs, sourcemaps, or client-side configuration need static analysis.
when-to-use: Use when a live web page may reference JavaScript files that expose routes, APIs, or client-side config.
allowed-tools:
  - js_collect
  - js_analyze
  - http_probe
  - http_request
  - todo_write
---

# js_collect

Use `js_collect` against a page URL to extract script `src` values. Set `download: true` only when you need bounded body previews for immediate analysis.

Use for:

- discovering bundled JS files referenced by a page
- preparing inputs for `js_analyze`
- finding source map references and route/API bundles
- collecting front-end evidence without browser automation

Recommended flow:

1. Start from a confirmed live page from `http_probe`.
2. Run `js_collect` with a small `limit`.
3. Analyze returned script URLs with `js_analyze`.
4. If assets are large or bundled, save evidence or use targeted `http_request` previews.

Keep collection bounded. Do not crawl recursively with this skill.
