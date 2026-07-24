---
name: validation_dispatch
description: Guidance for converting suspected findings and validation_plan fields into non-destructive next tool calls, blocked reasons, and finding_state updates after scanner normalization or manual hypothesis generation.
when-to-use: Use after vuln_engine or scanner tools return findings that are not already validated or blocked.
allowed-tools:
  - validation_dispatch
  - finding_update
  - http_request
  - web_xss_scan
  - sqlmap_scan
  - evidence_search
  - evidence_read
---

# validation_dispatch

Use `validation_dispatch` to convert one or more findings into concrete next tool calls.

Use for:

- turning suspected findings into safe baseline and validation calls
- identifying missing prerequisites such as authorization or test accounts
- producing a deterministic queue for the LLM or UI to review
- updating findings to `triaged` or `blocked` before execution

Inputs:

1. `finding` or `findings` directly from a scanner/vuln_engine result.
2. `finding_set_id` or `finding_set_path` from evidence.
3. `confirm_authorized=true` when target scope and authorization are clear.
4. `test_accounts_confirmed=true` only after two controlled test accounts exist for IDOR checks.

Output:

1. `next_tool_calls`: exact tool name and arguments to execute next.
2. `blocked`: validation actions that require authorization, test accounts, or would damage availability/data.
3. Updated findings with `finding_state.state` set to `triaged` or `blocked`.

After executing returned tool calls, review evidence and call `finding_update`:

1. `validated` for confirmed findings with linked evidence.
2. `false_positive` when validation disproves the signal.
3. `blocked` when authorization/scope/tooling prevents non-destructive validation.
4. `reported` only after the finding is included in a final report.

Non-destructive PoC validation is allowed for in-scope findings, including upload checks and inert payloads. Do not dispatch actions that leave persistence, upload shells/backdoors, corrupt data, affect non-test users, or make the site unavailable.

When returned `next_tool_calls` include specialized validators, execute them one by one and review evidence between steps instead of running a blind batch.
