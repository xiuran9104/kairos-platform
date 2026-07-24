---
name: url_grep
description: Guidance for extracting URLs, hosts, IPv4 addresses, API paths, JavaScript assets, and endpoint-like indicators from text, workspace files, fetched URL content, logs, crawler output, or scanner output.
when-to-use: Use when pages, logs, JavaScript, or saved outputs may contain endpoints or indicators.
allowed-tools:
  - url_grep
  - url_extract
  - read_file
  - grep
  - http_request
---

# url_grep

Use `url_grep` or `url_extract` to turn unstructured content into candidate URLs, hosts, IPv4 addresses, and paths.

Use for:

- parsing crawler/scanner/stdout output
- extracting URLs from JS, HTML, logs, markdown, and saved evidence
- deduplicating candidate endpoints before probing
- turning free-form observations into structured recon inputs

Input choices:

1. `text` for already available snippets or tool output.
2. `path` for files inside the workspace.
3. `url` for remote content; this requires network permission.

Recommended flow:

1. Extract indicators.
2. Deduplicate and group by host/path prefix.
3. Feed HTTP(S) candidates into `http_probe`.
4. Feed JavaScript assets into `js_analyze`.

Do not assume every extracted secret-like or endpoint-like string is valid. Confirm with non-destructive follow-up checks.
