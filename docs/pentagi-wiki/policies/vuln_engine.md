---
name: vuln_engine
description: Guidance for normalizing scanner stdout, JSONL, XML-ish output, or saved tool artifacts into PentAGI findings with finding_state, evidence_refs, validation_kind, safe validation_plan, and UI-ready fields.
when-to-use: Use after scanner stdout/json/xml is available and the UI or LLM loop needs structured findings plus next validation steps.
allowed-tools:
  - evidence_search
  - evidence_read
  - vuln_engine
  - validation_dispatch
  - finding_update
  - read_file
  - todo_write
---

# vuln_engine

Use `vuln_engine` to convert scanner output into structured findings. It does not execute scanners. Each finding has `finding_id`, `finding_state`, `validation_plan`, and `auto_validation`. Use `validation_dispatch` to turn findings into the next concrete tool calls, then use `finding_update` after reviewing validation evidence.

Use for:

- turning nuclei, dalfox, nikto, sslscan, testssl, sqlmap, or other scanner output into consistent findings
- attaching evidence refs to findings
- generating safe validation plans and stop conditions
- driving UI tables grouped by target, severity, confidence, and state
- preparing findings for report review

Evidence storage:

1. HTTP exchanges are stored under `.pentagi/evidence/http_exchange/`.
2. CLI tool runs are stored under `.pentagi/evidence/tool_run/`.
3. Normalized finding sets are stored under `.pentagi/evidence/finding_set/`.
4. Per-run artifacts are stored under `.pentagi/runs/{run_id}/`, including command metadata and stdout/stderr for CLI runs.
5. A line-delimited index is appended at `.pentagi/evidence/index.jsonl` with `id`, `kind`, `path`, `run_id`, `tool_call_id`, `run_dir`, `source_tool`, `target`, `status`, `success`, and `finding_count`.
6. External Web CLI wrappers inject a temporary local Rust recorder proxy through `HTTP_PROXY` and `HTTPS_PROXY`.
7. HTTPS interception uses PentAGI's generated CA at `.pentagi/ca/pentagi-ca.pem` and records decrypted request and response headers/bodies when the client accepts the CA.
8. Large HTTP bodies are externalized under `.pentagi/runs/{run_id}/bodies/`; evidence keeps `body_preview`, `body_path`, `body_sha256`, and size metadata.
9. Tool results and scanner findings may include `evidence_refs`; each ref includes the evidence `run_id`, `tool_call_id`, and related run artifact paths when available.
10. Redaction is disabled; stored evidence keeps raw headers, body previews/artifacts, stdout, and stderr.

Finding fields are intended for Web UI:

1. `title`
2. `severity`
3. `confidence`
4. `tool`
5. `target`
6. `template_id`
7. `evidence`
8. `evidence_refs`
9. `source`
10. `validation_kind`
11. `validation_plan`
12. `auto_validation`
13. `finding_id`
14. `finding_state`

Validation plans:

1. `sql_injection` recommends bounded `http_request` baseline/proof and `sqlmap_scan` with `confirm_authorized=true`; collect enough proof to confirm impact, but do not perform broad data dumping, OS shell, persistence, or destructive file operations.
2. `xss` recommends `http_request` response comparison and `web_xss_scan`; inert PoC payloads are allowed, but do not steal tokens or affect non-test users.
3. `path_traversal` recommends proof-file checks and scoped sensitive-file confirmation when the engagement allows it; avoid broad harvesting.
4. `file_upload` allows inert upload PoC files such as `pentagi-poc.txt` or clearly marked non-executing test content; do not upload web shells, persistence, or anything that degrades availability.
5. `idor` requires exactly two authorized test accounts and only A-to-B/B-to-A checks against those accounts.
6. `ssrf` and `csrf` plans require scoped, non-destructive validation and must stop before internal network scanning, amplification, or real-user impact.

Recommended flow:

1. Run a bounded scanner wrapper.
2. Pass scanner stdout or saved output path into `vuln_engine`.
3. Review and deduplicate findings.
4. Call `validation_dispatch` with the findings. Set `confirm_authorized=true` only when authorization is clear; set `test_accounts_confirmed=true` only after two test accounts exist.
5. Execute `validation_dispatch.next_tool_calls` in order when scope, authorization, and permissions allow it.
6. Use `finding_update` to move the finding to `validated`, `false_positive`, or `blocked` after reviewing evidence.
7. Display findings grouped by target, severity, confidence, and `finding_state.state`.
8. Use `evidence_search` for list/search views over `.pentagi/evidence/index.jsonl`.
9. Use `evidence_read` with `evidence_refs.id` or `evidence_refs.path` for raw request/response, tool output, or normalized finding-set details.
10. Use `tool_call_id` to correlate a UI-visible LLM tool call with the exact `tool_run`, `http_exchange`, and `finding_set` records created by that call.

Do not mark findings as validated just because a scanner emitted them. Validation requires reviewed evidence and a `finding_update` state change.
