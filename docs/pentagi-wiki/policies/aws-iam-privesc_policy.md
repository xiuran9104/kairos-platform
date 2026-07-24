---
name: aws-iam-privesc_policy
description: PentAGI safety policy for Aws Iam Privesc. AWS IAM 权限提升专项方法论。当已获取 AWS 凭据并需要提升权限、发现当前 IAM 用户/角色权限有限需要横向或纵向提权、或需要分析 IAM Policy 寻找提权路径时使用。覆盖 46 个 AWS 服务的提权技术，包括 PassRole 滥用、AssumeRole 链式提权、Lambda/EC2/ECS 计算服务提权、以及 NotAction 隐式权限利用
when-to-use: Use when Aws Iam Privesc methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# Aws Iam Privesc — PentAGI Safety Policy

> Policy compiled from `cloud/aws-iam-privesc/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `cloud/aws-iam-privesc/SKILL.md` for knowledge reference
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

Source: `cloud/aws-iam-privesc/SKILL.md`
