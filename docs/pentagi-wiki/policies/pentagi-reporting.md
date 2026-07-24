---
name: pentagi-reporting
description: Policy for PentAGI finding validation, evidence normalization, Burp-style HTTP request/response capture, report generation, and LLMWiki history updates.
when-to-use: Use when producing or improving security assessment reports.
tags: [pentagi, report, evidence, findings, wiki, burp]
related: [policies/pentagi-authorized-pentest, policies/pentagi-js-api-analysis, tools/waf_observation_plan]
---

# PentAGI Reporting

Use this policy when producing or improving security assessment reports.

## Finding Criteria

Only include findings that are:

- in scope
- validated
- useful to the target owner
- backed by evidence
- reproducible without unnecessary destructive actions

Low-value noise such as generic CORS, CAPTCHA, or non-impactful XSS should not be reported unless there is a concrete business impact.

## Evidence Format

For HTTP issues, include raw Burp Repeater-ready records:

- `burp_request_raw`: complete raw HTTP request starting with `METHOD /path HTTP/1.1`, including `Host`, relevant headers, blank line, and body when applicable.
- `burp_response_raw`: raw HTTP response excerpt starting with `HTTP/1.1 <status>`, relevant headers, blank line, and sanitized body excerpt.
- timestamp
- account used, if applicable

Use structured method/path/header fields only as optional metadata. The final report must contain raw copy-pastable HTTP blocks.

## Data Handling

- Do not include secrets, full tokens, database dumps, or real user data.
- Mask sensitive values.
- Prefer minimal proof over large extraction.
- Use test-owned accounts and records for authorization checks.

## Wiki Update

After reporting, update:

- `wiki/history/`
- relevant `wiki/playbooks/`
- relevant `wiki/tools/`
- relevant `wiki/vulns/` when a safe reusable pattern exists
