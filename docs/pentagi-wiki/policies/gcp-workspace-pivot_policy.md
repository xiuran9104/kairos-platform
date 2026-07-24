---
name: gcp-workspace-pivot_policy
description: PentAGI safety policy for Gcp Workspace Pivot. GCP 到 Google Workspace 的穿越攻击方法论。当已获取 GCP Service Account 或 Project 权限并发现目标组织使用 Google Workspace、需要从云平台穿越到企业邮件/文档/管理控制台、或发现 Domain-Wide Delegation 配置时使用。覆盖 Domain-Wide Delegation 滥用、OAuth 范围利用、Workspac
when-to-use: Use when Gcp Workspace Pivot methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# Gcp Workspace Pivot — PentAGI Safety Policy

> Policy compiled from `cloud/gcp-workspace-pivot/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `cloud/gcp-workspace-pivot/SKILL.md` for knowledge reference
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

Source: `cloud/gcp-workspace-pivot/SKILL.md`
