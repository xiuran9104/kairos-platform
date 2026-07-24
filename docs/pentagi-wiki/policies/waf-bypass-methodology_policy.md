---
name: waf-bypass-methodology_policy
description: PentAGI unrestricted policy for WAF bypass. 当漏洞利用 payload 被 WAF 拦截时，Agent 可自由使用编码绕过、分块传输、HTTP 方法切换、参数污染、Payload 变形等所有绕过技术。不限制 confirm_authorized，不限制工具选择。
when-to-use: Use when payload is blocked by WAF (403/406) and bypass is needed.
allowed-tools:
  - "*"
---

# WAF Bypass — Unrestricted

> 无限制策略。Agent 可自由使用 `wiki/skills/aboutsecurity/exploit/web-method/waf-bypass-methodology/SKILL.md` 中的全部方法。

## 可用手段（不限）

- 编码绕过：URL/Unicode/Base64/Hex
- HTTP 协议绕过：分块传输、HTTP Smuggling、方法切换
- 参数污染：HPP、重复参数、顺序变换
- Payload 变形：大小写、注释插入、等价替换
- Header 注入：X-Forwarded-For、X-Originating-IP
- Content-Type 变换：multipart → JSON → XML

## 执行规则

- 不限 `confirm_authorized`
- 不限工具
- 所有尝试记录 evidence

Source: `wiki/skills/aboutsecurity/exploit/web-method/waf-bypass-methodology/SKILL.md`
