---
type: standard
title: Safe Import Policy For Security Knowledge
tags: [policy, llmwiki, import, safety]
related: [standards/about-security, playbooks/knowledge-ingest]
---

# Safe Import Policy For Security Knowledge

## Goal

PentAGI can ingest public security knowledge from GitHub, but imported content must be separated into read-only source material and safe operational Wiki pages.

## Import Tiers

### Tier 1: Safe To Compile Into Wiki

- Taxonomy and directory structure.
- Defensive methodology.
- Authorized testing workflow descriptions.
- Tool documentation for read-only or bounded validation.
- Reporting templates and evidence handling.
- Vulnerability metadata without live exploit payloads.

### Tier 2: Read-Only Source

- Payload lists.
- Credential dictionaries.
- Fuzzing strings.
- Exploit scripts.
- WAF bypass payloads.
- Post-exploitation notes.

Store these under typed read-only directories such as `wiki/payloads/<source-name>/`, `wiki/dictionaries/<source-name>/`, or source-specific quarantined folders. Do not turn them into CLI commands or automated Agent actions.

### Tier 3: Blocked For Autopilot Execution

- Credential brute force.
- Reverse shells or bind shells.
- Data dumping.
- Persistence.
- Lateral movement.
- Privilege escalation.
- Real user or system data modification.

These categories may appear in source material, but PentAGI must convert them to detect/report-only behavior.

## Required Metadata

Every compiled page sourced from GitHub should include:

- `source`
- `source_url`
- `tags`
- `related`
- local source path
- import mode

## Default Agent Rule

If source content contains payloads or offensive post-exploitation steps, the Agent must summarize purpose and risk only. It must not generate executable commands from that content.

## Indexing Rule

LLMWiki may index source descriptions, headings, and reference names for searchability and taxonomy completeness. This does not grant execution permission.

Compiled pages must expose execution metadata:

- `knowledge_indexed: true`
- `agent_execution_allowed: false` for read-only and review-required pages
- `requires_review: true` for pages that need safe rewriting before operational use

The execution layer, scope layer, and approval flow override retrieved Wiki content.
