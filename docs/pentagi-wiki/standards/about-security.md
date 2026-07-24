---
type: standard
title: AboutSecurity Knowledge Source
tags: [github, knowledge-source, methodology, pentest]
source: wiki/sources/aboutsecurity/README.zh-CN.md
source_url: https://github.com/wgpsec/AboutSecurity/blob/master/README.zh-CN.md
related: [playbooks/knowledge-ingest, playbooks/web-basic, standards/safe-import-policy]
---

# AboutSecurity Knowledge Source

## Purpose

AboutSecurity is a structured security knowledge base designed for AI Agent consumption. PentAGI uses it as a methodology index and taxonomy seed, not as an unrestricted payload or exploit source.

## Imported Data

- Source metadata: `wiki/sources/aboutsecurity/`
- Local copy: `wiki/sources/aboutsecurity/README.zh-CN.md`
- Classified content manifest: `wiki/sources/aboutsecurity/MANIFEST.txt`
- Original GitHub manifest: `wiki/sources/aboutsecurity/original-github-manifest.txt`
- Skills format source: `wiki/sources/aboutsecurity/skills-README.md`
- Contribution/spec source: `wiki/sources/aboutsecurity/CONTRIBUTING.md`
- Root contents metadata: `wiki/sources/aboutsecurity/contents-root.json`
- Repository metadata: `wiki/sources/aboutsecurity/repo.json`
- Import record: `wiki/sources/aboutsecurity/IMPORT.md`
- Import mode: content-only GitHub raw/API fetch
- Git metadata: not imported

## Classified Scope

The imported repository is classified into PentAGI wiki directories for traceability and offline lookup:

- Skills: `wiki/skills/aboutsecurity/`
- Claude-style source skills: `wiki/skills/aboutsecurity/`
- Vulnerability data: `wiki/vulns/aboutsecurity/`
- Tool scripts: `wiki/tools/aboutsecurity/scripts/`
- Documentation/playbooks: `wiki/playbooks/aboutsecurity/docs/`
- Assets: `wiki/assets/aboutsecurity/`
- Dictionaries: `wiki/dictionaries/aboutsecurity/`
- Payloads: `wiki/payloads/aboutsecurity/`

PentAGI must treat these imported directories as source material. The Agent should read policy-gated `wiki/` pages first. Direct source reads are allowed for classification and documentation, but payloads, dictionaries, exploit scripts, and post-exploitation material must not be converted into automatic commands.

## Useful Taxonomy

- `Skills/`: structured scenario methods.
- `recon/`: passive information collection, subdomain discovery, JavaScript API extraction.
- `exploit/auth/`: JWT, OAuth/SSO, IDOR, CSRF, Cookie analysis.
- `exploit/web-method/`: injection, SSRF, SSTI, upload, deserialization, WAF detection.
- `exploit/network-service/`: service-oriented testing for SMB, FTP, SMTP, DNS, LDAP, and similar services.
- `tool/`: tool usage knowledge for fscan, nuclei, sqlmap, msfconsole, ffuf, hashcat, and similar tools.
- `Vuln/`: product vulnerability data by AI, cloud, middleware, network, and web applications.
- `general/`: reporting, supply-chain audit, and mobile backend API analysis.

## Skill Format Notes

See [[standards/about-security-skills]] for how AboutSecurity structures `SKILL.md`, `references/`, and Agent-facing skill discovery.

## Restricted Categories

The following categories stay read-only or policy-gated and must not become automatic execution steps:

- Credential dictionaries.
- Payload collections.
- Exploit scripts.
- WAF bypass payloads.
- Post-exploitation, lateral movement, malware, evasion, persistence, credential extraction, and privilege escalation topics.

## PentAGI Rules

1. Always check `scope.yaml` before using knowledge from this source.
2. Prefer safe methodology pages over direct payload data.
3. Convert only destructive or out-of-scope actions into safe alternatives:
   - WAF bypass -> bounded in-scope bypass validation with recorded request/response evidence; avoid traffic spikes.
   - Brute force -> controlled `srctest` account checks only.
   - Shell/RCE -> non-interactive proof-only validation.
   - SQL dump -> minimal scoped proof only; avoid broad harvesting and real user data.
   - File upload -> inert PoC file only; no shells, persistence, or availability impact.
   - Post-exploitation -> detect/report only.
4. Write verified lessons back to `wiki/playbooks/lessons.md`.

## Agent Routing Hints

- Web/API target: start with [[playbooks/web-basic]], then consult recon, auth, and web-method taxonomy.
- JavaScript/API analysis: map to recon and mobile backend API workflows.
- Authentication findings: map to auth taxonomy, but use controlled `srctest` accounts only.
- SQLi signal: use [[policies/sqlmap_scan]] only with non-dumping proof settings.
- Tool gap: use PentAGI capability coverage and install-plan tools, not ad hoc installation commands.
