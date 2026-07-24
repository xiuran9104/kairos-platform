---
name: supply-chain-audit_policy
description: PentAGI safety policy for Supply Chain Audit. 供应链安全审计与攻击。当指纹识别发现 WordPress/Jenkins/Struts/Django 等已知框架、或发现 /package.json /composer.json /package-lock.json /Gemfile 等依赖声明文件时使用。也适用于发现 CI/CD 构建系统（Jenkins/GitLab CI/OpsFlow）、私有镜像仓库（Harbor/Nexus/Verdac
when-to-use: Use when Supply Chain Audit methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Supply Chain Audit — PentAGI Safety Policy

> Policy compiled from `exploit/advanced/supply-chain-audit/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/advanced/supply-chain-audit/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 无 confirm_authorized 的主动利用
- 破坏性 payload（DROP/DELETE/rm -rf）
- 直接执行 OS 命令
- 写入 webshell 或后门文件

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

Source: `exploit/advanced/supply-chain-audit/SKILL.md`
