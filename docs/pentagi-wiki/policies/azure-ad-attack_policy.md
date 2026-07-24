---
name: azure-ad-attack_policy
description: PentAGI safety policy for Azure Ad Attack. Azure AD / Entra ID 攻击方法论。当目标使用 Microsoft 365/Azure 云环境、发现 Azure AD 认证流程、或获取到 Azure 凭据时使用。覆盖初始访问（Password Spray/Phishing）、令牌窃取、Service Principal 滥用、条件访问绕过、跨租户攻击
when-to-use: Use when Azure Ad Attack methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# Azure Ad Attack — PentAGI Safety Policy

> Policy compiled from `cloud/azure-ad-attack/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `cloud/azure-ad-attack/SKILL.md` for knowledge reference
- Use allowed PentAGI tools only within authorized scope
- Record all findings as evidence via `evidence_search` / evidence write tools
- Stop and request human review when observations are ambiguous
- 所有主动操作必须确认 `confirm_authorized=true`
- 未经明确授权的目标不得执行任何测试

## Blocked Operations

- 创建后门/持久化资源
- 跨账户横向移动
- 修改 IAM/CAM 策略
- 删除 CloudTrail/审计日志

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

Source: `cloud/azure-ad-attack/SKILL.md`
