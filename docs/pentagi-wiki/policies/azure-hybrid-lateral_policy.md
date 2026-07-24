---
name: azure-hybrid-lateral_policy
description: PentAGI safety policy for Azure Hybrid Lateral. Azure Cloud 到本地 AD 的横向移动方法论。当已获取 Azure/Entra ID 权限并需要穿越到本地 Active Directory 环境、发现目标使用 Azure AD Connect/Cloud Sync/PTA/Federation 等混合身份架构、或需要利用 PRT/Certificate/Cookie 等凭据进行 Cloud-to-OnPrem 横向时使用。覆盖 14 
when-to-use: Use when Azure Hybrid Lateral methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# Azure Hybrid Lateral — PentAGI Safety Policy

> Policy compiled from `cloud/azure-hybrid-lateral/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `cloud/azure-hybrid-lateral/SKILL.md` for knowledge reference
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

Source: `cloud/azure-hybrid-lateral/SKILL.md`
