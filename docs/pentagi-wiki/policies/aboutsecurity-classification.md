---
type: skill-catalog
name: aboutsecurity-classification
title: AboutSecurity Skill Classification
description: Classification rules for deciding which AboutSecurity materials can become safe PentAGI policy wrappers, wiki summaries, or read-only references.
when-to-use: Use when importing, compiling, or reviewing AboutSecurity skill content for PentAGI.
source: wiki/skills/aboutsecurity
tags: [skills, aboutsecurity, codex, llmwiki, safety]
related: [standards/safe-import-policy, standards/about-security-skills]
---

# AboutSecurity Skill Classification

## Purpose

AboutSecurity contains a broad set of `SKILL.md` files. PentAGI keeps the imported source under `wiki/skills/aboutsecurity/`, keeps safety guardrails under `wiki/policies/`, and exposes only safe workflow entry points to the agent.

Do not copy every upstream skill into execution guidance. Some skills contain brute force, evasion, post-exploitation, persistence, shell, dumping, or lateral movement procedures that must remain read-only.

## Storage Model

| Layer | Path | Contents | Agent Use |
|---|---|---|---|
| Imported skill content | `wiki/skills/aboutsecurity/` | Full upstream skill content classified by type | Read-only source material |
| Supporting content | `wiki/vulns/aboutsecurity/`, `wiki/payloads/aboutsecurity/`, `wiki/dictionaries/aboutsecurity/` | Imported vulnerability notes, payloads, and dictionaries | Read-only source material |
| Wiki catalog | `wiki/skills/`, `wiki/policies/` | Classification, allowed workflows, and policy guardrails | Retrieval and planning |
| Wrapper policies | `wiki/policies/` | Small safe wrappers for PentAGI planning | Interactive task guidance |

## Classification

### Policy-Wrapper Candidates

These can be represented as policy wrappers because they guide bounded analysis, reporting, or environment-aware planning:

- `general/report-generate`
- `general/judge-pentest`
- `general/local-resources`
- `general/efficiency-rules`
- `recon/js-api-extract`
- `recon/passive-recon`
- `recon/target-profiling`
- `ai-security/agent-security`
- `ai-security/mcp-security`
- `ai-security/prompt-injection`
- `ai-security/prompt-leak`
- `code-audit/java/*`
- `code-audit/php/*`
- `dfir/emergency-response`
- `threat-intel/ioc-analysis`

### Wiki-Compile Only

These are useful for playbook logic but should be rewritten into safe summaries before use:

- `tool/nmap-scan`
- `tool/httpx-probe`
- `tool/ffuf-fuzz`
- `tool/dirsearch-fuzz`
- `tool/katana-crawl`
- `tool/gau-urlfetch`
- `tool/subfinder-recon`
- `tool/dnsx-resolve`
- `tool/tlsx-probe`
- `tool/nikto-scan`
- `tool/nuclei-scan`
- `exploit/auth/idor-methodology`
- `exploit/auth/cookie-analysis`
- `exploit/auth/csrf-methodology`
- `exploit/web-method/api-fuzz`
- `exploit/web-method/graphql-methodology`
- `exploit/web-method/information-disclosure-methodology`
- `exploit/web-method/sql-injection-methodology`
- `exploit/web-method/ssrf-methodology`
- `exploit/web-method/xxe-injection-methodology`

Compile rules:

- Keep detection, reproduction, and evidence collection.
- Remove destructive methods, bypass-to-success loops, shell payloads, broad brute force, and data dumping.
- Convert exploit steps into proof-only checks with approval gates.

### Raw-Only Skills

These categories must not be copied into policy wrappers and must not become automated playbooks:

- `evasion/*`
- `malware/*`
- `lateral/*`
- `postexploit/*`
- `tool/hydra-brute`
- `tool/zombie-brute`
- `tool/spray-dir-brute`
- `tool/hashcat-crack`
- `tool/john-crack`
- `tool/cobalt-strike`
- `tool/mythic-c2`
- `tool/responder-poison`
- `tool/impacket-toolkit`
- `tool/msf-oneshot`
- `exploit/web-method/webshell-deploy`
- `exploit/web-method/webshell-management`
- `exploit/web-method/waf-bypass-methodology`
- `exploit/web-method/command-injection-methodology`
- cloud or network skills that include privilege escalation, persistence, credential abuse, or lateral movement

## PentAGI Policy Strategy

PentAGI should keep wrapper policies in `wiki/policies/`, not upstream offensive skills verbatim or project-local `.codex/skills` files.

Recommended policy pages:

- [[policies/pentagi-authorized-pentest]]: scope, policy, queue, report, Wiki update workflow.
- [[policies/pentagi-js-api-analysis]]: JS/HTML collection and static endpoint analysis.
- [[policies/pentagi-safe-tool-planning]]: choose locally installed tools through PentAGI coverage and doctor checks.
- [[policies/pentagi-reporting]]: evidence, Burp-style request/response records, finding validation.

## Update Process

1. Refresh imported source from the GitHub archive.
2. Regenerate `wiki/sources/aboutsecurity/MANIFEST.txt`.
3. Run classification review.
4. Compile only allowed or sanitized content into `wiki/`.
5. Keep stable wrappers under `wiki/policies/`.
