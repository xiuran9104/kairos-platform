//! Model-facing operational guidance for recipe-backed security tools.
//!
//! Registry descriptions stay concise for search and administration. This
//! module enriches the schema sent to the model with selection criteria,
//! evidence expectations and parameter semantics.

use serde_json::Value;

pub(super) fn enhance_tool_schema(mut schema: Value) -> Value {
    let Some(name) = schema
        .pointer("/function/name")
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        return schema;
    };

    if let Some(guidance) = tool_usage_guidance(&name) {
        let description = schema
            .pointer("/function/description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        schema["function"]["description"] = if description.is_empty() {
            Value::String(guidance.into())
        } else {
            Value::String(format!("{description}\n使用说明：{guidance}"))
        };
    }

    if let Some(properties) = schema
        .pointer_mut("/function/parameters/properties")
        .and_then(Value::as_object_mut)
    {
        for (parameter, property) in properties {
            let current = property
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if let Some(guidance) = parameter_guidance(&name, parameter) {
                property["description"] = Value::String(if current.trim().is_empty() {
                    guidance.into()
                } else {
                    format!("{}；{}", current.trim(), guidance)
                });
            }
        }
    }
    schema
}

fn tool_usage_guidance(name: &str) -> Option<&'static str> {
    match name {
        "hakrawler" | "katana" | "web_crawl" | "web_url_collect" => Some(
            "用于已授权 Web 目标的递归攻击面发现。输入根 URL，产出页面、API、JS、表单和带参数 URL；先运行它建立端点地图，再把隐藏参数交给 arjun/x8，把具体输入点交给专项验证器。应携带当前登录 Cookie，并覆盖重定向后的同源页面；发现不等于漏洞验证。",
        ),
        "arjun" | "x8" => Some(
            "针对单个已发现端点探测未公开的 GET/POST 参数，输入必须是具体端点而非站点根域。分别保留方法、Content-Type、请求体和登录态；新参数要进入 SQLi、XSS、SSRF、文件路径和业务逻辑验证队列。",
        ),
        "paramspider" | "gau" | "waybackurls" => Some(
            "从公开历史数据补充带参数 URL，适合主动爬虫后扩大攻击面。输出必须先做同源/Scope 过滤、去重并验证可达性，再按端点与参数扇出专项验证，不能直接作为漏洞证据。",
        ),
        "nuclei" | "web_vuln_scan" | "afrog" => Some(
            "用于已知模板、组件暴露和常见错误配置的广度扫描。应扫描爬虫发现的根 URL与关键子路径，登录区需传 Cookie/Header；按技术栈选择 tags/templates。命中后用 HTTP 重放或专项工具复现，阴性结果不能替代端点参数、越权和业务逻辑测试。",
        ),
        "sqlmap" | "sqlmap_scan" | "sqli_validate" => Some(
            "仅对具体输入点验证 SQL 注入。GET 参数保留完整查询串；POST/JSON 提供原始 body 和 Content-Type；认证区传 Cookie，必要时用 * 标记唯一注入点。先用非破坏性 batch/level 2-3 验证布尔、错误和时间差异，阴性但手工差异明显时再提高 level/risk。不得对无参数站点根 URL 空扫。",
        ),
        "dalfox" | "web_xss_scan" | "xss_validate" | "xsser" => Some(
            "对具体参数、表单或 DOM sink 验证反射型、存储型和 DOM XSS。携带登录 Cookie，先识别 HTML/属性/脚本/URL 上下文，再选择无害标记；仅字符串反射不等于脚本可执行。存储型必须包含写入与回读两个步骤。",
        ),
        "http-framework-test" | "http_request" | "traffic_replay" => Some(
            "用于精确复现和业务逻辑差分。保持同一端点，只改变一个变量（身份、对象 ID、HTTP 方法、价格/数量/状态字段），比较状态码、响应结构和服务端副作用。优先重放已捕获请求，保留 Cookie、Authorization、CSRF 和 Content-Type。",
        ),
        "command_injection_validate" | "commix" => Some(
            "只对命令、网络诊断、文件转换等可疑参数做无害标记或受控时间差验证。输入必须是具体端点和参数，保留请求方法与认证状态；普通 5xx、单次超时或工具报错不能作为命令执行证据。",
        ),
        "file_inclusion_validate" | "dotdotpwn" => Some(
            "对 file/path/page/template/download 等具体参数验证路径穿越或文件包含。使用无害且内容可识别的读取目标，测试编码与路径规范化变体，并区分应用模板内容、统一错误页与真实文件读取。",
        ),
        "file_upload_validate" => Some(
            "对具体上传表单提交无害标记文件并尝试从返回路径或可预测存储路径回读。必须保留字段名、文件名、Content-Type、Cookie、CSRF 和附加表单字段；仅返回上传成功不等于任意文件上传或代码执行。",
        ),
        "csrf_validate" => Some(
            "先捕获已登录状态下的真实敏感请求，识别 CSRF token、SameSite 和 Origin/Referer 校验；随后仅移除或替换防护字段并重放，确认服务端副作用。页面存在表单或缺少 token 本身不能证明 CSRF。",
        ),
        "wpscan" => Some(
            "仅在指纹确认 WordPress 后使用。先枚举核心、主题和插件版本，再按已发现组件做漏洞模板验证；认证扫描要传 Cookie。版本匹配只形成候选，最终结论需要可复现证据。",
        ),
        "wapiti" | "nikto" => Some(
            "作为 Web 广度扫描补充，覆盖常见配置、危险文件和基础输入点。应使用爬虫得到的有效基址并携带认证状态；结果用于生成验证队列，不得用单一扫描器阴性结果宣告站点安全。",
        ),
        "record_finding" => Some(
            "仅在漏洞已由当前 Run 的 Evidence 复现确认后调用。每个不同根因/端点单独记录；描述必须包含请求差异、实际响应或副作用，evidence_ids 必须直接支撑结论。",
        ),
        _ => None,
    }
}

fn parameter_guidance(tool: &str, parameter: &str) -> Option<&'static str> {
    match (tool, parameter) {
        (_, "url" | "target") => Some(
            "使用已授权且可达的具体目标；专项 Web 验证优先传完整端点和查询参数，不要只传站点根地址",
        ),
        (_, "cookies" | "cookie") => Some(
            "传入当前身份的完整 Cookie 头；多身份差分时每次只替换目标身份 Cookie并保留其他会话字段",
        ),
        (_, "headers" | "header") => Some(
            "保留 Authorization、Content-Type、Origin/Referer 等原请求语义；不要把敏感值写入最终报告",
        ),
        (_, "data" | "body" | "form") => Some(
            "保持原始编码和字段结构；一次只改变待验证字段，JSON 请求需同时保留 application/json",
        ),
        ("sqlmap" | "sqlmap_scan", "level") => {
            Some("先用 2-3；存在稳定差异但未命中时再升到 4-5，避免无差别高请求量扫描")
        }
        ("sqlmap" | "sqlmap_scan", "risk") => {
            Some("默认低风险验证；只有在授权允许且低风险技术不足时提高，避免破坏性 payload")
        }
        ("nuclei" | "web_vuln_scan" | "afrog", "tags") => Some(
            "根据已识别技术栈或漏洞类别选择，如 cve、exposure、misconfig、wordpress；不要只跑单一高危标签",
        ),
        ("nuclei" | "web_vuln_scan" | "afrog", "template" | "templates") => {
            Some("优先使用已维护模板目录；自定义模板路径必须位于受控工具目录且与当前技术栈匹配")
        }
        ("dalfox" | "web_xss_scan" | "xss_validate" | "xsser", "parameter" | "param") => {
            Some("填写爬虫、表单或 JS 分析实际发现的参数名；每个输入点独立验证")
        }
        (_, "additional_args") => {
            Some("仅补充配方未覆盖且已确认安全的非交互参数；禁止拼接 shell、重定向或越权目标")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn enriches_core_tool_selection_and_parameter_semantics() {
        let schema = enhance_tool_schema(json!({
            "type": "function",
            "function": {
                "name": "sqlmap",
                "description": "SQL 注入检测",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": {"type": "string"},
                        "cookies": {"type": "string", "description": "Cookie"},
                        "level": {"type": "integer"}
                    }
                }
            }
        }));
        let description = schema["function"]["description"].as_str().unwrap();
        assert!(description.contains("POST/JSON"));
        assert!(
            schema["function"]["parameters"]["properties"]["url"]["description"]
                .as_str()
                .unwrap()
                .contains("具体目标")
        );
        assert!(
            schema["function"]["parameters"]["properties"]["cookies"]["description"]
                .as_str()
                .unwrap()
                .starts_with("Cookie；")
        );
        assert!(
            schema["function"]["parameters"]["properties"]["level"]["description"]
                .as_str()
                .unwrap()
                .contains("2-3")
        );
    }

    #[test]
    fn leaves_unknown_tools_unchanged() {
        let schema = json!({"function":{"name":"custom","description":"custom"}});
        assert_eq!(enhance_tool_schema(schema.clone()), schema);
    }
}
