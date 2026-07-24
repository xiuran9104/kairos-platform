# http_request

Makes one structured HTTP(S) request and records request/response evidence.

Use for:

- baseline request capture
- response comparison
- header and MIME checks
- safe replay of known requests
- fetching proof URLs

Evidence is stored as an HTTP exchange with request, response, timestamps, and body artifacts when large.

Do not use this as a generic bypass or brute-force loop. Prefer specialized wrappers such as `file_upload_validate` or `sqlmap_scan` when validation needs safety gates.

Typical follow-ups:

- Use `evidence_search` with the returned `run_id` to link the exchange.
- Use `waf_observation_plan` when the response suggests a WAF/CDN/security-control block.
- Use `validation_dispatch` specialized validators when a finding has a known `validation_kind`.
