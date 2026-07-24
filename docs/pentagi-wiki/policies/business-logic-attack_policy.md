---
name: business-logic-attack_policy
description: PentAGI safety policy for Business Logic Attack. Web 应用业务逻辑漏洞检测与利用。当目标有支付/交易/订单/优惠券/积分/余额/充值/转账/购物车等商业功能时使用。当发现用户注册/登录/短信验证码/邮箱验证/密码重置等账户功能时使用。当 API 请求中出现价格(price/amount)、数量(quantity/count)、订单号(order_id)、优惠码(coupon/promo)等业务参数时使用。业务逻辑漏洞不需要技术漏洞（SQLi/
when-to-use: Use when Business Logic Attack methodology is triggered and active testing may occur.
allowed-tools:
  - http_request
  - evidence_search
  - evidence_read
  - waf_bypass_engine
  - waf_observation_plan
  - todo_write
---

# Business Logic Attack — PentAGI Safety Policy

> Policy compiled from `exploit/web-method/business-logic-attack/SKILL.md`. This policy restricts unsafe automation while preserving the knowledge value of the underlying skill.

## Allowed Operations

- Consult the full methodology in `exploit/web-method/business-logic-attack/SKILL.md` for knowledge reference
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

Source: `exploit/web-method/business-logic-attack/SKILL.md`
