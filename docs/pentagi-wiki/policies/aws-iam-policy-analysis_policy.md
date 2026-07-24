---
name: aws-iam-policy-analysis_policy
description: PentAGI safety policy for Aws Iam Policy Analysis. AWS IAM / Resource Policy 分析方法论。当目标涉及 AWS 云服务且提供了 IAM Policy、Resource Policy、Lambda 代码、CloudFormation 模板等策略文件时使用。覆盖 IAM Policy 危险模式识别、Resource Policy Principal 分析、Condition 键绕过模式、服务信任关系图推导。发现 AWS 策略文件
when-to-use: Use when Aws Iam Policy Analysis methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# Aws Iam Policy Analysis — PentAGI Safety Policy

> Policy compiled from `cloud/aws-iam-policy-analysis/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `cloud/aws-iam-policy-analysis/SKILL.md` for knowledge reference
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

Source: `cloud/aws-iam-policy-analysis/SKILL.md`
