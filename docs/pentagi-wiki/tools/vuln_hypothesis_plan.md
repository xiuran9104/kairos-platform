# vuln_hypothesis_plan

Turns discovered routes, endpoints, technologies, auth states, and observations into structured vulnerability hypotheses.

This is a planning tool. It does not attack targets. It emits:

- `validation_kind`
- severity hypothesis
- oracle checks
- validation_plan
- guardrails
- evidence_refs

Common generated hypothesis kinds:

- `file_upload`
- `idor`
- `sql_injection`
- `xss`
- `ssrf`
- `path_traversal`
- `csrf`

Use it after crawl, JS/API extraction, parameter discovery, or manual observations. Feed the output into `validation_dispatch` to produce concrete safe next tool calls.

Good inputs include:

- route lists from `web_crawl`
- API paths from `js_analyze`
- parameterized URLs from `url_grep`
- technologies from `web_fingerprint`
- auth-state observations such as anonymous, user A, user B, admin, or frontend-hidden routes

The output is hypothesis quality. It should guide validation, not become a report finding until evidence confirms impact.
