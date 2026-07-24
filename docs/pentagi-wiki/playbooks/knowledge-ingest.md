---
type: playbook
title: GitHub Security Knowledge Ingest
tags: [llmwiki, github, ingest, compile]
related: [standards/safe-import-policy, standards/about-security]
---

# GitHub Security Knowledge Ingest

## Purpose

Use this playbook when importing public security knowledge into PentAGI LLMWiki.

## Workflow

1. Fetch content-only data from GitHub raw/API endpoints.
2. Store source metadata under `wiki/sources/<source-name>/`.
3. Classify source content into typed wiki directories such as `wiki/tools/`, `wiki/vulns/`, `wiki/playbooks/`, `wiki/skills/`, `wiki/payloads/`, `wiki/dictionaries/`, and `wiki/assets/`.
4. Do not clone `.git` metadata into the project.
5. Classify the material using [[standards/safe-import-policy]].
6. Compile only safe methodology, taxonomy, and operating rules into `wiki/`.
7. Keep payloads, dictionaries, exploit scripts, and post-exploitation material read-only or policy-gated.
8. Update `wiki/index.md` with new standards, tools, playbooks, or vulnerability taxonomy pages.

## Source Layout

```text
wiki/sources/<source-name>/
├── README.md
├── repo.json
├── IMPORT.md
└── MANIFEST.md

wiki/skills/<source-name>/
wiki/vulns/<source-name>/
wiki/tools/<source-name>/
wiki/playbooks/<source-name>/
wiki/payloads/<source-name>/      # read-only
wiki/dictionaries/<source-name>/  # read-only
```

## Compilation Targets

- Source overview: `wiki/standards/<repo>.md`
- Safe workflow: `wiki/playbooks/<topic>.md`
- Tool notes: `wiki/tools/<tool>.md`
- Vulnerability taxonomy: `wiki/vulns/<category>.md`

## Safety Checks

- No `.git` directories.
- No automatic use of credential dictionaries.
- No automatic use of payload lists.
- No shell, persistence, lateral movement, or privilege escalation actions.
- All operational steps must remain scope-bound and auditable.
