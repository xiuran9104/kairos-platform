---
type: standard
title: AboutSecurity Skill Format
tags: [github, skills, methodology, agent]
source: wiki/sources/aboutsecurity/skills-README.md
source_url: https://github.com/wgpsec/AboutSecurity/blob/master/skills/README.md
related: [standards/about-security, standards/safe-import-policy, playbooks/knowledge-ingest]
---

# AboutSecurity Skill Format

## Purpose

AboutSecurity uses `SKILL.md` files as structured methodology units for AI Agents. PentAGI imports the format rules and taxonomy, but does not import payload references into executable plans.

## Format Model

- Each skill is a directory with a `SKILL.md`.
- `SKILL.md` is the decision/methodology layer.
- `references/` contains detailed material and is loaded only when needed.
- `evals/` or benchmark data can be used to evaluate skill quality.

## Agent Consumption Model

AboutSecurity distinguishes between:

- Maintainer layout: `skills/<category>/<skill-name>/SKILL.md`
- Agent layout: `.claude/skills/<skill-name>/SKILL.md`

PentAGI does not need symlinks. It keeps imported source files under typed wiki directories such as `wiki/skills/aboutsecurity/` and keeps execution guardrails in `wiki/policies/`.

## PentAGI Adaptation

When importing a skill:

1. Keep original skill source under `wiki/skills/<source-name>/`.
2. Extract trigger conditions, decision tree, safe validation flow, and output expectations.
3. Remove or quarantine payload lists and exploit scripts.
4. Convert unsafe action details into detect/report-only notes.
5. Link resulting pages from `wiki/index.md`.

## Useful Categories For PentAGI

- `recon`: information gathering and JavaScript/API discovery.
- `exploit/auth`: authentication and authorization review.
- `exploit/web-method`: web vulnerability methodology.
- `exploit/network-service`: service enumeration methodology.
- `tool`: current tool usage and output interpretation.
- `general`: reporting and API review patterns.
- `code-audit`: white-box review patterns when source code is available.

## Restricted Categories

Keep these raw-only unless converted to defensive or report-only form:

- `postexploit`
- `lateral`
- `evasion`
- `malware`
- payload references
- credential dictionaries
- exploit scripts

## PentAGI Skill Page Requirements

Compiled PentAGI skill pages should include:

- Trigger conditions.
- Required scope checks.
- Safe workflow phases.
- Allowed tools and capabilities.
- Analyzer output signals.
- Stop conditions.
- Report evidence requirements.
- Related Wiki links.
