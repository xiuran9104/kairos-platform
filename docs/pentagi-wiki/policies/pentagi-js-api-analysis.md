---
name: pentagi-js-api-analysis
description: Policy for safe JS/HTML static analysis, frontend route discovery, API endpoint extraction, and token/client-side logic review in an authorized PentAGI assessment.
when-to-use: Use when analyzing downloaded HTML, JavaScript bundles, source maps, frontend routes, API clients, token handling, or client-side authorization logic.
tags: [pentagi, javascript, html, api, static-analysis, frontend, token]
related: [skills/aboutsecurity/recon/js-api-extract, policies/pentagi-authorized-pentest, policies/pentagi-reporting]
---

# PentAGI JS And API Static Analysis

Use this policy when the task is to analyze downloaded HTML, JavaScript bundles, source maps, frontend routes, API clients, token handling, or client-side authorization logic.

## Safe Scope

- Static analysis of local files is allowed.
- Fetching target pages or scripts must respect engagement scope and rate limits.
- Delete, update, patch, upload, administrative, and state-changing endpoints are inventory-only unless explicitly approved and mapped to test-owned data.
- Do not test real user data or system records.

## Analysis Steps

1. Collect local HTML, JS, source maps, manifests, and route chunks.
2. Extract API base URLs, relative paths, GraphQL endpoints, WebSocket endpoints, auth headers, token storage logic, feature flags, and role checks.
3. Classify endpoints by method and risk.
4. Mark state-changing endpoints as approval-required.
5. Check for client-side-only authorization, predictable IDs, insecure token storage, debug flags, exposed keys, and environment leakage.
6. Generate proof-only validation tasks for read-only endpoints.
7. Write findings and endpoint inventory back to LLMWiki.

## Output Format

Produce:

- endpoint inventory
- auth/token flow summary
- candidate logic flaws
- skipped state-changing endpoints
- safe validation plan
- report-ready evidence

## Source Material

Use `wiki/skills/aboutsecurity/recon/js-api-extract/SKILL.md` only as read-only reference. Do not copy unsafe validation instructions blindly.
