---
name: evidence_search
description: Search, read, and reindex PentAGI evidence records by run_id, tool_call_id, target, source tool, kind, evidence id, or evidence path when previous HTTP exchanges, tool runs, or normalized findings must be reviewed, linked, or reported.
when-to-use: Use when previous tool output, HTTP exchanges, or normalized findings need to be reviewed or linked.
allowed-tools:
  - evidence_search
  - evidence_read
  - evidence_reindex
---

# evidence_search

Use `evidence_search` first to query `.pentagi/evidence/index.jsonl`.

Use for:

- finding the exact HTTP request/response behind a claim
- linking `evidence_refs` to `finding_update` and reports
- correlating LLM tool calls with saved run artifacts
- recovering raw stdout/stderr, scanner findings, or body artifacts
- rebuilding the evidence index after migration or manual repair

Common filters:

1. `run_id` to retrieve all records from one tool execution.
2. `tool_call_id` to retrieve evidence created by one LLM tool call.
3. `kind` as `tool_run`, `http_exchange`, or `finding_set`.
4. `source_tool` to focus on one PentAGI wrapper.
5. `target_contains` for a URL, host, or path substring.
6. `query` for a broad case-insensitive search over index rows.

Use `evidence_read` after search:

1. Prefer `id` when available.
2. Use `path` when opening an `evidence_refs.path`.
3. Increase `max_bytes` only when the evidence document is expected to be large.

Run artifacts:

1. Evidence refs and index rows may include `run_dir`.
2. CLI tool runs may include `command_path`, `stdout_path`, `stderr_path`, and `metadata_path`.
3. HTTP exchanges may include `exchange_path`, `request_body_path`, and `response_body_path`.
4. Finding sets may include `finding_set_path`.
5. Large bodies live under `.pentagi/runs/{run_id}/bodies/` and include `body_sha256` metadata in the evidence JSON.
6. Use `run_id` to retrieve all evidence records from one execution.
7. Use `tool_call_id` when linking a chat transcript tool-call event to saved evidence.

The raw evidence is unredacted. Treat headers, bodies, stdout, and stderr as sensitive.

Maintenance:

1. Use `evidence_reindex` with `dry_run: true` to inspect how many evidence records would be indexed.
2. Use `evidence_reindex` with `backup_existing: true` to rebuild `.pentagi/evidence/index.jsonl` after migration, manual edits, or index corruption.
3. Reindexing reads evidence JSON files and skips malformed records.

Do not paste raw cookies, bearer tokens, credentials, or sensitive business data into reports. Summarize impact and link evidence refs instead.
