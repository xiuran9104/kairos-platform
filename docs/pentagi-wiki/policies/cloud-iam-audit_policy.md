---
name: cloud-iam-audit_policy
description: PentAGI safety policy for Cloud Iam Audit. 云 IAM 权限审计与提权。当获取了云平台凭据（AWS AK/SK、Azure SPN、GCP SA、腾讯云 SecretId/SecretKey）需要评估权限范围和提权路径时使用。覆盖 AWS/Azure/GCP/腾讯云的 IAM/CAM 策略分析、常见提权路径（PassRole、AssumeRole、Lambda/SCF 提权）、跨账号攻击、CloudTrail/CloudAudit 规避。发
when-to-use: Use when Cloud Iam Audit methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# Cloud Iam Audit — PentAGI Safety Policy

> Policy compiled from `cloud/cloud-iam-audit/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `cloud/cloud-iam-audit/SKILL.md` for knowledge reference
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

Source: `cloud/cloud-iam-audit/SKILL.md`
