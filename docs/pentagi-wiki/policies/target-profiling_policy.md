---
name: target-profiling_policy
description: PentAGI safety policy for Target Profiling. 目标全景画像与攻击面分析。当需要系统性整理目标资产、生成结构化目标档案、或在多人协作渗透中需要共享目标信息时使用。侧重于分析和报告，而非扫描本身。适合在 recon-full 之后对数据进行深度分析
when-to-use: Use when Target Profiling methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - dns_lookup
  - whois_lookup
  - http_probe
  - evidence_search
  - todo_write
---

# Target Profiling — PentAGI Safety Policy

> Policy compiled from `recon/target-profiling/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `recon/target-profiling/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 端口扫描（需 nmap_scan policy 授权）
- 主动漏洞利用
- 社会工程学信息收集

## Escalation Path

If the methodology requires blocked operations:
1. Stop automated execution
2. Document the specific need and target
3. Request explicit human authorization with scope boundaries
4. Wait for approval before proceeding

## Evidence Requirements

- Record every probe attempt with request/response evidence
- Link findings to the source methodology path
- Mark unverified observations as `needs_review`

---

Source: `recon/target-profiling/SKILL.md`
