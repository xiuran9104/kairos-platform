---
name: cloud-metadata_policy
description: PentAGI safety policy for Cloud Metadata. 云元数据利用。当通过 SSRF 或已获取的 shell 可以访问云实例元数据服务时使用。覆盖 AWS/Azure/GCP/阿里云/腾讯云的元数据端点、IAM/CAM 凭据提取、IMDSv2 绕过、从元数据到云服务枚举的完整攻击链。发现任何 SSRF 场景、内网可访问 169.254.169.254 或 100.100.100.200 的场景都应使用此技能
when-to-use: Use when Cloud Metadata methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# Cloud Metadata — PentAGI Safety Policy

> Policy compiled from `cloud/cloud-metadata/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `cloud/cloud-metadata/SKILL.md` for knowledge reference
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

Source: `cloud/cloud-metadata/SKILL.md`
