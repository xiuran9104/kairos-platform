---
name: attack-planning_policy
description: PentAGI safety policy for Attack Strategy Planning. LLM 驱动的智能渗透测试入口——先全量下载 JS 并交给 LLM 深度分析理解目标架构/业务/鉴权，再结合传统信息收集生成精准攻击计划。覆盖从授权确认到漏洞验证的完整攻击生命周期。
when-to-use: Use when Attack Planning methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - http_probe
  - dns_lookup
  - whois_lookup
  - evidence_search
  - evidence_write
  - todo_write
  - validation_dispatch
  - vuln_hypothesis_plan
---

# Attack Planning — PentAGI Safety Policy

> Policy compiled from `playbooks/attack-planning.md`. This policy restricts unsafe automation while preserving the full methodology.

## Core Principle

**Understand first, attack second.** Modern pentesting prioritizes downloading all JS files and feeding them to LLM for deep analysis before generating attack plans. This policy gates the full attack lifecycle.

## Allowed Operations

- Full JS file download and collection (passive, read-only)
- LLM analysis of downloaded JS code
- Allowed PentAGI tools within authorized scope only
- Record all findings as evidence via evidence tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- Any active testing on out-of-scope targets
- Port scanning without explicit authorization (requires recon-full policy)
- Active exploitation without explicit authorization
- Social engineering information gathering
- Credential brute forcing on production accounts
- WAF bypass payloads without bounded in-scope validation
- Data exfiltration beyond minimal proof
- Shell/RCE without non-interactive proof-only validation

## Escalation Path

If the methodology requires blocked operations:
1. Stop automated execution
2. Document the specific need and target
3. Request explicit human authorization with scope boundaries
4. Wait for approval before proceeding

## JS Download Boundaries

JS 下载阶段**不需要设限**——这是信息收集，不是攻击。原则就是能拿多少拿多少：

- **全量下载**：目标主站、子域名、CDN 上的 JS 全部拉下来。CDN 上的 JS 也是目标的代码，一样会暴露 API 路由、鉴权逻辑和内部域名
- **不理会 robots.txt**：授权渗透测试不受 robots.txt 约束，直接下载
- **Source Map 优先**：有 map 文件就下载，可以还原完整前端源码
- **Wayback Machine**：历史 JS 也要，可能包含已删除但未下线的 API
- **递归收集**：JS 里动态 import 的 chunk、Webpack 分包、微前端子应用，全部递归下载
- **并发不限**：只要目标扛得住，并发下载即可，不要自限 10 个
- **唯一硬性限制**：只下载授权范围内的目标。不在授权文件里的域名，不管是不是 CDN 都不碰

## LLM Analysis Boundaries

- JS code sent to LLM must not contain real user data (sanitize first)
- API endpoints discovered through JS analysis are STILL subject to scope validation
- LLM-generated attack plans are advisory only — execution requires tool-level authorization

## Evidence Requirements

- Record all JS download sources and results
- Record LLM analysis outputs as structured evidence
- Link findings to specific JS file sources
- Mark unverified LLM predictions as `needs_review`
- Track endpoint verification status (verified / unverified / false_positive)

## Phase-Specific Rules

### Phase 0 (Authorization)
- Scope snapshot must be immutable once confirmed
- Domain resolution re-validation before any network access

### Phase 1-2 (JS Download + LLM Analysis)
- JS 下载是全量信息收集，不是主动攻击——不惜一切代价拿全
- CDN、第三方托管、Source Map、历史版本全部下载，不设并发限制
- 爬取认证区域需确认有合法测试账号，但不阻止信息收集
- Source Map 恢复的源码与原始 JS 同等对待，属于目标代码

### Phase 3 (Traditional Recon)
- Delegated to individual skill policies (recon-full, passive-recon, etc.)
- Each sub-phase requires its own policy gate

### Phase 4 (LLM Attack Planning)
- LLM is advisory only
- Attack plans must be reviewed before execution
- High-risk plans require human approval

### Phase 5 (Execution)
- Each attack path requires its own tool authorization
- High-risk operations require explicit approval via approval API

---

Source: `playbooks/attack-planning.md`
