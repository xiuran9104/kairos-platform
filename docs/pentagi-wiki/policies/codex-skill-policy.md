---
type: policy
name: codex-skill-policy
title: PentAGI Wrapper Policy
description: Safety policy for deciding what PentAGI content can be exposed as wiki policy wrappers.
when-to-use: Use when creating, reviewing, or approving PentAGI wrapper policies and their allowed workflows.
tags: [policies, skills, safety, pentagi]
related: [policies/aboutsecurity-classification, standards/safe-import-policy]
---

# PentAGI Wrapper Policy

## Decision

PentAGI should use `wiki/policies/` wrapper pages for safe workflow guidance. Do not place the entire AboutSecurity `skills/` tree into executable or auto-selected guidance.

## Why

Policy wrappers are retrieved during planning and can steer execution. Raw offensive skills can steer the agent toward unsafe automation if copied directly.

## Allowed Content

- Authorized testing scope checks.
- Environment discovery and local tool capability planning.
- Static analysis of local files, downloaded JS, HTML, API routes, and logs.
- Read-only reconnaissance.
- Vulnerability validation that is proof-only and scoped.
- Report writing and evidence normalization.
- Wiki update and knowledge compilation instructions.

## Disallowed Content

- Credential brute force or password spraying.
- Reverse shells, bind shells, web shells, payload delivery, or persistence.
- WAF bypass loops that continue until success.
- Dumping database tables, file reads, secrets harvesting, or broad data extraction.
- Lateral movement, privilege escalation, post-exploitation, or C2 operation.
- Modifying real users, system accounts, production data, or non-test records.

## Implementation Rule

Wrapper policies live in `wiki/policies/<policy-name>.md`.

Each PentAGI wrapper policy must:

- Require explicit authorization and scope.
- Prefer PentAGI tools over raw shell execution.
- Route command execution through existing policy gates.
- Stop on high-risk actions and produce an approval item instead.
- Write findings and evidence back to LLMWiki through the PentAGI workflow.

## Knowledge Versus Execution

PentAGI should keep imported security knowledge searchable. Source descriptions, headings, and reference names may be indexed so the Agent can find relevant methodology and understand capability gaps.

Indexing knowledge is not execution approval. Pages marked `agent_execution_allowed: false` or `requires_review: true` are retrieval material only. The Agent must convert them into a reviewed safe playbook, tool page, vulnerability note, or approval item before any operational use.

Executor-level hard blocks remain authoritative even when a Wiki page contains restricted terms such as brute force, shells, dumping, persistence, privilege escalation, or lateral movement.
