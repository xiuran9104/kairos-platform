---
name: cicd-pipeline-attack_policy
description: PentAGI safety policy for Cicd Pipeline Attack. CI/CD 流水线与供应链攻击方法论。当发现目标使用 GitHub Actions/Jenkins/GitLab CI/CircleCI/Terraform Cloud/Atlantis 等 CI/CD 系统、需要测试流水线安全性、或发现 .github/workflows/Jenkinsfile/.gitlab-ci.yml 等配置文件时使用。覆盖 PPE（Poisoned Pipeline E
when-to-use: Use when Cicd Pipeline Attack methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - todo_write
---

# Cicd Pipeline Attack — PentAGI Safety Policy

> Policy compiled from `cloud/cicd-pipeline-attack/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `cloud/cicd-pipeline-attack/SKILL.md` for knowledge reference
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

Source: `cloud/cicd-pipeline-attack/SKILL.md`
