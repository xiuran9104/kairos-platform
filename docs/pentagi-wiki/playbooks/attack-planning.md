---
type: playbook
title: 攻击思路规划 — LLM 驱动的智能渗透测试入口
tags: [attack-planning, llm, js-analysis, recon, pentest-strategy, 攻击规划, 渗透入口]
related:
  - playbooks/web-basic
  - playbooks/waf-aware-validation
  - skills/aboutsecurity/recon/js-api-extract/SKILL.md
  - skills/aboutsecurity/recon/recon-full/SKILL.md
  - skills/aboutsecurity/recon/passive-recon/SKILL.md
  - skills/aboutsecurity/recon/target-profiling/SKILL.md
  - policies/js-api-extract_policy
  - references/aboutsecurity/Checklist.zh-cn.md
---

# 攻击思路规划 — LLM 驱动的智能渗透测试入口

## 0. 核心思路：先理解，再攻击

**旧模式**：子域名 → 端口扫描 → 目录爆破 → 逐点测试（线性、低效、容易遗漏）

**新模式**：
```text
全量 JS 下载 → LLM 深度分析 → 理解架构/业务/鉴权 → 精准攻击规划 → 自动+手动执行 → 迭代优化
```

现在的 Web 应用绝大多数是 SPA/前后端分离架构，前端 JS bundle 里包含了：
- **完整 API 路由表**（比目录扫描高效 10 倍）
- **鉴权逻辑**（token 传递方式、角色判断、权限校验）
- **业务逻辑**（支付流程、订单状态机、用户关系）
- **内部域名和 IP**（测试环境、内部 API、WebSocket）
- **密钥和凭据**（API Key、第三方服务 Token、调试接口）
- **技术栈和版本**（框架、组件库、构建工具链）

**核心原则**：让 LLM 先理解目标，再生成攻击计划，而非盲目扫描。

---

## 1. Phase 0：授权与范围确认

在所有操作之前，必须确认：

```text
1. 授权文件和有效期
2. 目标范围（域名、IP、CIDR、URL、端口白名单）
3. 禁止目标（黑名单和例外规则）
4. 测试类型限制（只读/写入/删除/提权）
5. 测试账号（若提供）
6. 时间窗口和并发限制
7. Token/费用预算
```

**硬性规则**：
- 范围外目标绝不触碰
- 授权过期立即停止
- 禁止操作由代码强制，不可通过对话绕过

---

## 2. Phase 1：全量 JS 资源下载（现代渗透第一步）

> 这是整个攻击规划的**数据基础**。JS bundle 质量直接决定后续 LLM 分析的有效性。

### 2.1 JS 文件发现与收集

#### 从页面入口收集
```bash
# 抓取主页所有 JS 引用
curl -sL "$TARGET" | grep -oE '(src|href)="[^"]*\.js[^"]*"' | sed 's/.*="\(.*\)"/\1/' | sort -u > js_urls.txt

# 递归：抓取所有 HTML 页面中的 JS 引用
curl -sL "$TARGET" | grep -oP '(?:src|href|url|content)\s*[=:]\s*["\x27]([^"\x27]*\.js[^"\x27]*)["\x27]' | sort -u >> js_urls.txt

# 从 sitemap 收集
curl -s "$TARGET/sitemap.xml" | grep -oP 'https?://[^\s<]+' | while read url; do
    curl -sL "$url" | grep -oE '\.js[^"\x27]*' | sed "s|^|$url|" | sort -u >> js_urls.txt
done
```

#### 从 Webpack/构建工具清单收集
```bash
# Webpack manifest
curl -s "$TARGET/static/js/manifest.json"
curl -s "$TARGET/asset-manifest.json"
curl -s "$TARGET/webpack-manifest.json"
curl -s "$TARGET/build/asset-manifest.json"

# Vite manifest
curl -s "$TARGET/.vite/manifest.json"
curl -s "$TARGET/manifest.json"

# Next.js 构建清单
curl -s "$TARGET/_next/static/chunks/pages/_app-*.js" | tail -1
```

#### 从 Source Map 恢复完整源码
```bash
# 检查所有 JS 文件末尾的 sourceMappingURL
for js in $(cat js_urls.txt); do
    map_url=$(curl -sL "$js" | grep -oP '//#\s*sourceMappingURL=\K.*' | tail -1)
    if [ -n "$map_url" ]; then
        echo "Found sourcemap: $map_url"
        # Source Map 可以恢复完整的前端源码——价值极高
        curl -sL "$map_url" -o "$(basename "$map_url")"
    fi
done
```

#### 从 Wayback Machine 获取历史 JS
```bash
# 旧版 JS 可能包含已删除但未下线的 API 端点
curl -s "https://web.archive.org/cdx/search/cdx?url=$DOMAIN/*.js&output=text&fl=original&limit=100" | sort -u >> js_urls.txt
curl -s "https://web.archive.org/cdx/search/cdx?url=$DOMAIN/*.js.map&output=text&fl=original&limit=50" | sort -u
```

#### 递归深入：从 JS 中提取更多 JS
```bash
# JS 中动态 import 的路径
grep -rhoP '(?:import|require|importScripts)\s*\(?\s*["\x27]([^"\x27]+\.js[^"\x27]*)["\x27]' js_downloads/ | sort -u
# Webpack chunk 动态加载
grep -rhoP '["\x27]([a-zA-Z0-9_\-/]+\.chunk\.js)["\x27]' js_downloads/ | sort -u
```

### 2.2 批量下载

```bash
# 下载所有 JS 到本地目录
mkdir -p js_downloads
cat js_urls.txt | sort -u | while read url; do
    # 补全相对路径
    [[ "$url" != http* ]] && url="${TARGET%/}/$url"
    filename=$(echo "$url" | sed 's|https\?://||' | sed 's|/|_|g' | cut -c1-200)
    curl -sL --connect-timeout 5 -m 15 "$url" -o "js_downloads/$filename" && echo "OK: $url" || echo "FAIL: $url"
done

# 统计
echo "Total JS files downloaded: $(find js_downloads -type f -size +0c | wc -l)"
echo "Total size: $(du -sh js_downloads | cut -f1)"
```

### 2.3 初步过滤与分类

```bash
# 去除非 JS 内容（404 页面、HTML 重定向等）
find js_downloads -type f -exec file {} \; | grep -v "JavaScript\|JSON\|ASCII text" && echo "Found non-JS files, review and remove"

# 按大小分类
echo "Large bundles (>500KB):";   find js_downloads -type f -size +500k | sort
echo "Medium bundles (50-500KB):"; find js_downloads -type f -size +50k -size -500k | sort
echo "Small scripts (<50KB):";     find js_downloads -type f -size -50k | sort

# 识别第三方库（排除，优先分析业务代码）
find js_downloads -type f | while read f; do
    basename "$f" | grep -qiE 'jquery|bootstrap|lodash|moment|axios|vue\.|react\.|angular\.|swiper|echarts|chart\.|polyfill\.|core-js|regenerator|webpack\.runtime|chunk-vendors|vendor\.|vendors\.' && echo "THIRD_PARTY: $f" || echo "BUSINESS: $f"
done
```

---

## 3. Phase 2：LLM 深度分析 JS（核心差异化能力）

> **这是现代渗透最关键的一步**——把收集到的 JS 代码交给 LLM 进行深度分析，让 AI 帮你理解目标。

### 3.1 分批提交给 LLM 分析

由于 LLM 上下文限制，需要按优先级分批提交：

**第一批（高优先级）：业务代码中的核心发现**
```
分析以下前端 JS 代码片段，提取所有安全相关信息：

1. API 路由表（包括 HTTP 方法、路径、参数）
2. 鉴权逻辑（token 类型、存储方式、刷新机制、角色判断）
3. 敏感信息（API Key、密钥、内部域名、调试接口）
4. 权限校验逻辑（前端路由守卫、按钮级权限）
5. WebSocket/SSE 端点
6. 文件上传/下载逻辑
7. 第三方服务集成（支付、短信、邮件、OSS）
8. 隐藏的管理/调试功能
9. 业务逻辑漏洞线索（价格计算、状态流转、权限判断）
10. 技术栈和版本信息（框架、UI 库、构建工具）

输出格式：
- 每个发现标注置信度（高/中/低）
- 标注对应的 JS 文件名和行号（如果可定位）
- 标注是否可以立即验证
- 给出攻击建议和优先级
```

**第二批（中优先级）：路由和组件结构**
```
分析以下前端代码，提取：

1. 完整的页面路由表（公开页面 vs 需认证页面 vs 管理员页面）
2. 组件树结构，识别：
   - 管理后台组件（Admin/Dashboard/Manage/System 等前缀）
   - 用户权限组件（Role/Permission/Auth 相关）
   - 业务核心组件（Order/Pay/Transaction/User 等）
3. 状态管理（Vuex/Pinia/Redux）中的数据结构
4. 环境变量和构建配置
```

**第三批（低优先级）：第三方依赖**
```
分析以下依赖信息，提取：
1. 使用的第三方库及其版本（判断是否存在已知 CVE）
2. 构建工具和配置（Webpack/Vite/Next 配置泄露）
3. 开发依赖（可能暴露内部工具和流程）
```

### 3.2 LLM 自动提取结构化攻击面

让 LLM 生成结构化的攻击面清单：

```json
{
  "api_endpoints": [
    {
      "path": "/api/v1/user/profile",
      "method": "GET",
      "params": ["userId"],
      "auth_required": true,
      "risk": "IDOR: userId 可遍历",
      "confidence": "high",
      "source": "js_downloads/app.abc123.js:1245"
    }
  ],
  "auth_mechanism": {
    "type": "JWT Bearer Token",
    "storage": "localStorage",
    "refresh": "/api/v1/auth/refresh",
    "notes": "无 HttpOnly 标志，XSS 可窃取"
  },
  "sensitive_exposures": [
    {"type": "API Key", "value": "sk-***", "source": "js_downloads/app.abc123.js:5678", "severity": "critical"},
    {"type": "Internal Domain", "value": "admin.internal.example.com", "source": "js_downloads/config.xyz.js:123", "severity": "high"}
  ],
  "hidden_endpoints": [
    "/api/v1/admin/debug/sql",
    "/api/v1/internal/health"
  ],
  "tech_stack": {
    "framework": "Vue 3.4",
    "ui_library": "Ant Design Vue 4.x",
    "build_tool": "Vite 5.x",
    "state_management": "Pinia"
  },
  "attack_surface_priority": [
    "IDOR in user APIs (9 endpoints accept userId)",
    "File upload without type validation",
    "Admin API exposed without proper RBAC",
    "Debug endpoints discoverable"
  ]
}
```

### 3.3 与传统扫描结果的对比校准

将 LLM 从 JS 中提取的端点列表与目录扫描结果对比：

```bash
# LLM 提取的端点数 vs 目录扫描发现的端点数
echo "LLM extracted endpoints: $(cat llm_endpoints.txt | wc -l)"
echo "Directory brute-force found: $(cat dirscan_endpoints.txt | wc -l)"

# 重叠率分析
comm -12 <(sort llm_endpoints.txt) <(sort dirscan_endpoints.txt) | wc -l  # 重叠
comm -23 <(sort llm_endpoints.txt) <(sort dirscan_endpoints.txt) | head -20  # 仅 LLM 发现（目录扫描遗漏）
```

**结论**：通常 LLM 从 JS 提取的端点覆盖率是目录扫描的 5-10 倍，且发现者目录扫描无法触及的隐藏 API（如带参数的 POST 接口）。

---

## 4. Phase 3：传统信息收集（补充攻击面）

> JS 分析是主力，传统扫描用于补充——发现非 Web 服务和网络层攻击面。

### 4.1 子域名枚举

按 [[skills/aboutsecurity/recon/recon-full/SKILL.md]] 的 Phase 1 执行：
- `ksubdomain` 高速枚举
- 重点标注：`dev.`、`test.`、`staging.`、`api.`、`admin.`、`internal.`

### 4.2 端口扫描

按 [[skills/aboutsecurity/recon/recon-full/SKILL.md]] 的 Phase 2 执行：
- `naabu` 快速端口扫描（非 nmap）
- 高价值端口优先：80/443/8080/8443（Web）、3306/5432/6379/27017（数据库）、22/3389（远程管理）

### 4.3 存活检测与指纹识别

按 [[skills/aboutsecurity/recon/recon-full/SKILL.md]] 的 Phase 3-4 执行：
- `httpx` 存活检测
- 技术栈指纹 → 攻击策略映射

### 4.4 被动信息收集

按 [[skills/aboutsecurity/recon/passive-recon/SKILL.md]] 执行：
- FOFA / Quake / Hunter 多引擎交叉比对
- WHOIS、DNS 历史、GitHub 泄露搜索

---

## 5. Phase 4：LLM 综合攻击规划

> **将 Phase 2 的 JS 分析结果 + Phase 3 的传统扫描结果 + 漏洞知识库，全部交给 LLM，生成精准攻击计划。**

### 5.1 输入 LLM 的完整上下文

```
你是一个渗透测试专家。请基于以下信息生成攻击计划：

## 目标信息
- 主域名：xxx.com
- 技术栈：Vue 3.4 + Ant Design Vue + Vite 5（来自 JS 分析）
- 鉴权方式：JWT Bearer Token，localStorage 存储（来自 JS 分析）
- 服务器：Nginx 1.24，后端疑似 Go/Java（来自指纹识别）

## JS 分析发现（Phase 2）
[粘贴 LLM 提取的结构化攻击面 JSON]

## 传统扫描发现（Phase 3）
- 子域名：12 个（api.xxx.com, admin.xxx.com, dev.xxx.com...）
- 开放端口：80, 443, 8080（管理后台）, 22
- 指纹：Nginx 1.24, PostgreSQL 15, Redis 7

## 要求
请生成按优先级排序的攻击路径列表，每条攻击路径包含：
1. 攻击目标（具体端点/服务）
2. 攻击方法（工具和技术）
3. 前置条件（需要什么信息或权限）
4. 预期难度（低/中/高）
5. 预期影响（信息泄露/权限提升/数据篡改/命令执行）
6. 是否需要审批
7. 验证步骤（如何确认漏洞存在）

重点关注：
- JS 中提取的隐藏 API 端点
- 鉴权绕过机会
- IDOR 漏洞（JS 暴露的参数结构）
- 文件上传点
- 管理后台弱口令/默认凭据
- 开发/测试环境未授权访问
```

### 5.2 LLM 输出的攻击计划

LLM 按攻击链（Kill Chain）组织输出：

```text
攻击路径 1（高危，优先执行）：IDOR 获取任意用户信息
  ├─ 目标：/api/v1/user/profile?userId=
  ├─ 来源：JS bundle app.abc123.js L1245
  ├─ 方法：遍历 userId 参数（数字型，JS 中显示自增）
  ├─ 难度：低（仅需登录态）
  ├─ 影响：所有用户个人信息泄露
  ├─ 验证：对比两个不同 userId 的返回
  └─ 工具：http_request

攻击路径 2（高危）：管理后台未授权访问
  ├─ 目标：http://admin.xxx.com:8080
  ├─ 来源：JS 中硬编码的后台地址
  ├─ 方法：尝试默认凭据 + 目录扫描
  ├─ 难度：中（可能有弱口令）
  ├─ 影响：完全控制后台功能
  └─ 工具：dir_fuzz, http_probe

攻击路径 3（中危）：文件上传绕过
  ├─ 目标：/api/v1/upload/avatar
  ├─ 来源：JS bundle upload.xyz.js L234
  ├─ 方法：后缀名 Fuzz（JS 中只做前端校验）
  ├─ 难度：中
  ├─ 影响：任意文件上传 → RCE
  └─ 工具：手动测试 + Burp Suite

攻击路径 4（低危，信息收集）：.git 源码泄露
  ├─ 目标：https://xxx.com/.git/HEAD
  ├─ 来源：传统目录扫描
  ├─ 方法：GitHacker 下载
  ├─ 难度：低
  └─ 工具：githacker
```

### 5.3 攻击路径排序规则

| 优先级 | 条件 | 示例 |
|--------|------|------|
| 🔴 立即 | 无需登录即可触发的高危漏洞 | RCE、未授权管理后台、SQL 注入 |
| 🟠 高 | 登录后可触发的高价值漏洞 | IDOR（用户数据）、越权、文件上传 |
| 🟡 中 | 需特定条件但有明确路径 | 认证绕过、SSRF、XSS 链 |
| 🟢 低 | 信息收集类，无直接危害 | 源码泄露、路径发现、指纹识别 |

---

## 6. Phase 5：攻击执行

按照 Phase 4 生成的攻击计划，按优先级执行。

### 6.1 自动验证阶段

对 LLM 发现的高置信度漏洞直接验证：

```text
1. IDOR 批量测试：用 http_request 遍历 JS 中提取的所有对象 ID 参数
2. API 存活验证：对 JS 提取的所有端点做 HTTP 方法探测（GET/POST/PUT/DELETE）
3. 敏感信息确认：验证发现的 token/key 是否有效
```

### 6.2 手动验证阶段

对需要人工判断的漏洞：

```text
1. 认证绕过（需理解业务逻辑）
2. 支付逻辑漏洞（需理解价格计算）
3. 复杂 XSS/CSRF 链
4. 需要社会工程学配合的攻击
```

### 6.3 工具链映射

| 漏洞类型 | 自动工具 | 手动验证 |
|----------|----------|----------|
| IDOR | 自定义脚本遍历 | Burp Repeater 确认 |
| SQL 注入 | sqlmap | 手工 Payload |
| XSS | nuclei | 浏览器验证 |
| 文件上传 | 后缀 Fuzz 脚本 | 上传后的文件访问测试 |
| SSRF | nuclei + 自定义 | 带外 DNS 确认 |
| 认证绕过 | — | 全手动 |
| 逻辑漏洞 | — | 全手动 |

---

## 7. Phase 6：常规渗透测试清单（传统方法补充）

> JS 分析 + LLM 规划是主力突破方向，但以下传统测试点也不能遗漏。

按 [[references/aboutsecurity/Checklist.zh-cn.md]] 完整清单逐项检查：

### 7.1 Web 前台通用检查

**信息收集**
- [ ] 子域名枚举
- [ ] 真实 IP 搜集（CDN 绕过）
- [ ] 端口扫描（全端口 1-65535）
- [ ] IP 反查
- [ ] WHOIS 信息
- [ ] 目录扫描
- [ ] 组件版本识别

**敏感信息泄露**
- [ ] JS 敏感信息泄露（API Key、Token、内部域名）
- [ ] .git / .svn / .DS_Store 源码泄露
- [ ] .env 配置文件泄露
- [ ] WEB-INF/web.xml 信息泄露
- [ ] 前端注释中的敏感信息
- [ ] 报错页面信息泄露（中间件版本、路径）
- [ ] 目录浏览
- [ ] GitHub 泄露（搜索域名 + password/key/secret）

**认证与授权**
- [ ] 403 Bypass（协议/方法/Header/路径变体）
- [ ] CORS 配置错误
- [ ] JWT 攻击（无签名/弱密钥/密钥泄露）
- [ ] 越权（水平/垂直）
- [ ] 注册/登录/密码找回逻辑漏洞
- [ ] 验证码绕过/复用/爆破
- [ ] 短信轰炸

**注入类**
- [ ] SQL 注入（GET/POST/Cookie/Header）
- [ ] XSS（反射/存储/DOM）
- [ ] XXE
- [ ] SSTI
- [ ] 命令注入
- [ ] CRLF 注入

**文件操作**
- [ ] 任意文件上传
- [ ] 任意文件读取/下载
- [ ] 路径穿越
- [ ] 文件包含（LFI/RFI）

**业务逻辑**
- [ ] 支付金额篡改
- [ ] 订单遍历
- [ ] 优惠券/积分盗刷
- [ ] 并发竞态条件
- [ ] 批量操作无限制

### 7.2 主机与服务

- [ ] FTP 弱口令/匿名访问
- [ ] SSH 弱口令
- [ ] Redis 未授权访问
- [ ] MongoDB 未授权访问
- [ ] Elasticsearch 未授权访问
- [ ] Docker API 未授权访问
- [ ] 数据库弱口令（MySQL/PG/MSSQL/Oracle）
- [ ] 系统漏洞（MS17-010/CVE-2019-0708/Shellshock）

### 7.3 移动端/客户端

- [ ] API 接口漏洞（与后端通用）
- [ ] 客户端硬编码密钥
- [ ] 不安全的加密算法
- [ ] 二次打包漏洞
- [ ] 第三方 SDK 漏洞

### 7.4 云平台

按各云平台 policy 执行：
- [ ] AWS：S3 存储桶公开访问、IAM 权限、Lambda 注入
- [ ] 阿里云：OSS Bucket、RAM 策略、函数计算
- [ ] Azure：Blob Storage、AD 攻击
- [ ] GCP：Storage Bucket、IAM、Cloud Functions
- [ ] 华为云/天翼云

---

## 8. Phase 7：迭代与报告

### 8.1 迭代机制

```text
每次执行后的反馈循环：
  发现新端点/服务 → 更新 LLM 分析 → 重新规划 → 继续执行
  发现鉴权方式 → 更新绕过策略
  发现新子域名 → 递归执行 JS 下载+分析
```

### 8.2 关键看板

在整个攻击过程中持续追踪：

| 指标 | 说明 |
|------|------|
| API 端点覆盖率 | 已发现端点 / JS 中提取的端点总数 |
| 高危漏洞命中率 | 已验证高危 / LLM 预测的高危目标 |
| 攻击路径执行率 | 已执行路径 / 规划路径总数 |
| JS 分析覆盖率 | 已分析 JS 文件数 / 已下载 JS 文件数 |

### 8.3 报告输出

```text
1. 执行摘要（高危发现、受影响资产、攻击路径）
2. 漏洞详情（描述、复现步骤、PoC、修复建议）
3. 攻击路径图谱（从信息收集到漏洞利用的完整链路）
4. JS 分析价值报告（JS 发现了多少端点，目录扫描发现了多少）
5. 未完成项（因时间/权限/范围限制未执行的项目）
6. 修复优先级建议
```

---

## 9. 工具速查

| 阶段 | 工具 | 用途 |
|------|------|------|
| JS 下载 | curl + grep/sed | 全量 JS 收集 |
| JS 下载 | waybackurls | 历史 JS 获取 |
| JS 分析 | **LLM** | 深度代码理解、攻击面提取 |
| 子域名 | ksubdomain | 高速子域名枚举 |
| 端口 | naabu | 快速端口扫描 |
| 存活 | httpx | HTTP 服务探测 |
| POC | nuclei | 已知漏洞扫描 |
| 目录 | ffuf | 目录/文件爆破 |
| SQL | sqlmap | SQL 注入自动化 |
| 手动 | Burp Suite | HTTP 流量分析 |

---

## 10. 与传统渗透测试的关键差异

| 维度 | 传统渗透 | LLM 驱动渗透 |
|------|----------|-------------|
| 信息收集 | 目录扫描、爬虫 | **JS 全量下载 + LLM 理解** |
| 端点发现 | 字典爆破（覆盖率 ~30%） | **JS 静态分析（覆盖率 ~90%）** |
| 攻击面分析 | 人工经验判断 | **LLM 结构化提取 + 优先级排序** |
| 攻击规划 | 手动 checklist | **LLM 生成针对性攻击路径** |
| 业务逻辑理解 | 需要人工深入测试 | **JS 直接暴露业务逻辑** |
| 效率 | 慢，依赖测试人员经验 | **快，AI 批量分析 + 精准打击** |
| 遗漏率 | 高（隐藏 API、带参接口） | **低（JS 中几乎全部暴露）** |

---

## 11. 相关文档导航

- 完整 Web 测试清单 → [[references/aboutsecurity/Checklist.zh-cn.md]]
- JS API 提取技能 → [[skills/aboutsecurity/recon/js-api-extract/SKILL.md]]
- 全流程主动侦察 → [[skills/aboutsecurity/recon/recon-full/SKILL.md]]
- 被动情报收集 → [[skills/aboutsecurity/recon/passive-recon/SKILL.md]]
- 目标全景画像 → [[skills/aboutsecurity/recon/target-profiling/SKILL.md]]
- WAF 感知验证 → [[playbooks/waf-aware-validation]]
- 基础 Web 测试 → [[playbooks/web-basic]]
- 安全标准 → [[standards/about-security]]
