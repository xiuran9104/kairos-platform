---
type: index
name: policies-index
title: PentAGI Policies Index
description: Index of PentAGI policy guardrails for native tools and AboutSecurity-derived skills.
when-to-use: Use when locating safety policies and guardrails for skills, tools, or AboutSecurity methods.
tags: [policies, safety, index, pentagi]
related: [skills/aboutsecurity/index, policies/aboutsecurity-classification, standards/safe-import-policy]
---

# PentAGI Policies Index

This index lists policy guardrails. It is not an execution allowlist; scope, approval, and tool policy still override retrieved content.

## Native PentAGI Policies

| Policy | Name | Use |
|---|---|---|
| [[policies/aboutsecurity-classification]] | AboutSecurity Skill Classification | Use when importing, compiling, or reviewing AboutSecurity skill content for PentAGI. |
| [[policies/codex-skill-policy]] | Codex Skill Policy For PentAGI | Use when creating, reviewing, or approving PentAGI Codex skills and their allowed workflows. |
| [[policies/dir_fuzz]] | dir_fuzz | Use after a web host is confirmed live and path discovery is authorized. |
| [[policies/dns_lookup]] | dns_lookup | Use when domain ownership, host mapping, or DNS records guide target discovery. |
| [[policies/evidence_search]] | evidence_search | Use when previous tool output, HTTP exchanges, or normalized findings need to be reviewed or linked. |
| [[policies/http_probe]] | http_probe | Use after URLs or HTTP ports are known and the next step is web surface triage. |
| [[policies/js_analyze]] | js_analyze | Use after JavaScript content or asset URLs are available. |
| [[policies/js_collect]] | js_collect | Use when a live web page may reference JavaScript files that expose routes, APIs, or client-side config. |
| [[policies/nmap_scan]] | nmap_scan | Use after scope is confirmed and the next step is TCP port/service identification. |
| [[policies/pentagi-authorized-pentest]] | pentagi-authorized-pentest | Use when planning or running authorized PentAGI security testing workflows. |
| [[policies/pentagi-js-api-analysis]] | pentagi-js-api-analysis | Use when analyzing downloaded HTML, JavaScript bundles, source maps, frontend routes, API clients, token handling, or client-side authorization logic. |
| [[policies/pentagi-reporting]] | pentagi-reporting | Use when producing or improving security assessment reports. |
| [[policies/pentagi-safe-tool-planning]] | pentagi-safe-tool-planning | Use when deciding which CLI tools PentAGI should use. |
| [[policies/poc_review]] | poc_review | Use when exploit or POC code appears relevant but execution would be high risk. |
| [[policies/sqlmap_scan]] | sqlmap_scan | Use only when SQL injection testing is explicitly authorized and a concrete URL/request is in scope. |
| [[policies/url_grep]] | url_grep | Use when pages, logs, JavaScript, or saved outputs may contain endpoints or indicators. |
| [[policies/validation_dispatch]] | validation_dispatch | Use after vuln_engine or scanner tools return findings that are not already validated or blocked. |
| [[policies/vuln_engine]] | vuln_engine | Use after scanner stdout/json/xml is available and the UI or LLM loop needs structured findings plus next validation steps. |
| [[policies/web_crawl]] | web_crawl | Use when a live web app needs URL discovery before probing or analysis. |
| [[policies/web_fingerprint]] | web_fingerprint | Use after HTTP liveness is confirmed and the next step is identifying web technologies. |
| [[policies/web_tls_scan]] | web_tls_scan | Use when HTTPS services need certificate, protocol, or cipher triage. |
| [[policies/web_url_collect]] | web_url_collect | Use during Web recon when a domain is authorized and historical URLs may reveal routes or parameters. |
| [[policies/web_vuln_scan]] | web_vuln_scan | Use only when vulnerability scanning is explicitly authorized for the target URL. |
| [[policies/web_wordpress_scan]] | web_wordpress_scan | Use only after WordPress is identified on an in-scope URL. |
| [[policies/web_xss_scan]] | web_xss_scan | Use when a concrete in-scope URL or parameter needs XSS validation. |
| [[policies/whois_lookup]] | whois_lookup | Use for passive ownership and registration context before active testing. |

## AboutSecurity-Derived Policies

| Policy | Name | Use |
|---|---|---|
| [[policies/401-403-bypass_policy]] | 401-403-bypass_policy | Use when 401 403 Bypass methodology is triggered and active testing may occur. |
| [[policies/ai-infrastructure-attack_policy]] | ai-infrastructure-attack_policy | Use when AI Infrastructure Attack methodology is triggered and active testing may occur. |
| [[policies/aliyun-pentesting_policy]] | aliyun-pentesting_policy | Use when Aliyun Pentesting methodology is triggered and active testing may occur. |
| [[policies/android-app-pentesting_policy]] | android-app-pentesting_policy | Use when Android App Pentesting methodology is triggered and active testing may occur. |
| [[policies/api-fuzz_policy]] | api-fuzz_policy | Use when Api Fuzz methodology is triggered and active testing may occur. |
| [[policies/apt-emulation_policy]] | apt-emulation_policy | Use when Apt Emulation methodology is triggered and active testing may occur. |
| [[policies/aws-iam-policy-analysis_policy]] | aws-iam-policy-analysis_policy | Use when Aws Iam Policy Analysis methodology is triggered and active testing may occur. |
| [[policies/aws-iam-privesc_policy]] | aws-iam-privesc_policy | Use when Aws Iam Privesc methodology is triggered and active testing may occur. |
| [[policies/aws-pentesting_policy]] | aws-pentesting_policy | Use when Aws Pentesting methodology is triggered and active testing may occur. |
| [[policies/aws-post-exploit_policy]] | aws-post-exploit_policy | Use when Aws Post Exploit methodology is triggered and active testing may occur. |
| [[policies/azure-ad-attack_policy]] | azure-ad-attack_policy | Use when Azure Ad Attack methodology is triggered and active testing may occur. |
| [[policies/azure-hybrid-lateral_policy]] | azure-hybrid-lateral_policy | Use when Azure Hybrid Lateral methodology is triggered and active testing may occur. |
| [[policies/azure-pentesting_policy]] | azure-pentesting_policy | Use when Azure Pentesting methodology is triggered and active testing may occur. |
| [[policies/binary-exploitation-methodology_policy]] | binary-exploitation-methodology_policy | Use when Binary Exploitation Methodology methodology is triggered and active testing may occur. |
| [[policies/binary-exploitation-tools_policy]] | binary-exploitation-tools_policy | Use when Binary Exploitation Tools methodology is triggered and active testing may occur. |
| [[policies/browser-xterm-interaction_policy]] | browser-xterm-interaction_policy | Use when Browser Xterm Interaction methodology is triggered and active testing may occur. |
| [[policies/business-logic-attack_policy]] | business-logic-attack_policy | Use when Business Logic Attack methodology is triggered and active testing may occur. |
| [[policies/cache-poisoning-smuggling_policy]] | cache-poisoning-smuggling_policy | Use when Cache Poisoning Smuggling methodology is triggered and active testing may occur. |
| [[policies/cicd-pipeline-attack_policy]] | cicd-pipeline-attack_policy | Use when Cicd Pipeline Attack methodology is triggered and active testing may occur. |
| [[policies/clickjacking_policy]] | clickjacking_policy | Use when Clickjacking methodology is triggered and active testing may occur. |
| [[policies/cloud-aksk-exploit_policy]] | cloud-aksk-exploit_policy | Use when Cloud Aksk Exploit methodology is triggered and active testing may occur. |
| [[policies/cloud-iam-audit_policy]] | cloud-iam-audit_policy | Use when Cloud Iam Audit methodology is triggered and active testing may occur. |
| [[policies/cloud-metadata_policy]] | cloud-metadata_policy | Use when Cloud Metadata methodology is triggered and active testing may occur. |
| [[policies/command-injection-methodology_policy]] | command-injection-methodology_policy | Use when Command Injection Methodology methodology is triggered and active testing may occur. |
| [[policies/cookie-analysis_policy]] | cookie-analysis_policy | Use when Cookie Analysis methodology is triggered and active testing may occur. |
| [[policies/cors-misconfiguration_policy]] | cors-misconfiguration_policy | Use when Cors Misconfiguration methodology is triggered and active testing may occur. |
| [[policies/crypto-web-attack_policy]] | crypto-web-attack_policy | Use when Crypto Web Attack methodology is triggered and active testing may occur. |
| [[policies/csrf-methodology_policy]] | csrf-methodology_policy | Use when Csrf Methodology methodology is triggered and active testing may occur. |
| [[policies/ctf-ai-ml_policy]] | ctf-ai-ml_policy | Use when CTF AI Ml methodology is triggered and active testing may occur. |
| [[policies/ctf-crypto_policy]] | ctf-crypto_policy | Use when CTF Crypto methodology is triggered and active testing may occur. |
| [[policies/ctf-flag-hunting_policy]] | ctf-flag-hunting_policy | Use when CTF Flag Hunting methodology is triggered and active testing may occur. |
| [[policies/ctf-flag-verification_policy]] | ctf-flag-verification_policy | Use when CTF Flag Verification methodology is triggered and active testing may occur. |
| [[policies/ctf-forensics_policy]] | ctf-forensics_policy | Use when CTF Forensics methodology is triggered and active testing may occur. |
| [[policies/ctf-malware_policy]] | ctf-malware_policy | Use when CTF Malware methodology is triggered and active testing may occur. |
| [[policies/ctf-misc_policy]] | ctf-misc_policy | Use when CTF Misc methodology is triggered and active testing may occur. |
| [[policies/ctf-osint_policy]] | ctf-osint_policy | Use when CTF Osint methodology is triggered and active testing may occur. |
| [[policies/ctf-pwn_policy]] | ctf-pwn_policy | Use when CTF Pwn methodology is triggered and active testing may occur. |
| [[policies/ctf-reverse_policy]] | ctf-reverse_policy | Use when CTF Reverse methodology is triggered and active testing may occur. |
| [[policies/ctf-solve_policy]] | ctf-solve_policy | Use when CTF Solve methodology is triggered and active testing may occur. |
| [[policies/ctf-source-audit_policy]] | ctf-source-audit_policy | Use when CTF Source Audit methodology is triggered and active testing may occur. |
| [[policies/ctf-web-methodology_policy]] | ctf-web-methodology_policy | Use when CTF Web Methodology methodology is triggered and active testing may occur. |
| [[policies/ctf-web-recon_policy]] | ctf-web-recon_policy | Use when CTF Web Recon methodology is triggered and active testing may occur. |
| [[policies/dalfox-xss_policy]] | dalfox-xss_policy | Use when Dalfox Xss methodology is triggered and active testing may occur. |
| [[policies/deserialization-methodology_policy]] | deserialization-methodology_policy | Use when Deserialization Methodology methodology is triggered and active testing may occur. |
| [[policies/dirsearch-fuzz_policy]] | dirsearch-fuzz_policy | Use when Dirsearch Fuzz methodology is triggered and active testing may occur. |
| [[policies/disk-forensics-evasion_policy]] | disk-forensics-evasion_policy | Use when Disk Forensics Evasion methodology is triggered and active testing may occur. |
| [[policies/dns-pentesting_policy]] | dns-pentesting_policy | Use when Dns Pentesting methodology is triggered and active testing may occur. |
| [[policies/dnsx-resolve_policy]] | dnsx-resolve_policy | Use when Dnsx Resolve methodology is triggered and active testing may occur. |
| [[policies/docker-pentesting_policy]] | docker-pentesting_policy | Use when Docker Pentesting methodology is triggered and active testing may occur. |
| [[policies/efficiency-rules_policy]] | efficiency-rules_policy | Use when Efficiency Rules methodology is triggered and active testing may occur. |
| [[policies/elasticsearch-attack_policy]] | elasticsearch-attack_policy | Use when Elasticsearch Attack methodology is triggered and active testing may occur. |
| [[policies/emergency-response_policy]] | emergency-response_policy | Use when Emergency Response methodology is triggered and active testing may occur. |
| [[policies/expression-language-injection_policy]] | expression-language-injection_policy | Use when Expression Language Injection methodology is triggered and active testing may occur. |
| [[policies/ffuf-fuzz_policy]] | ffuf-fuzz_policy | Use when Ffuf Fuzz methodology is triggered and active testing may occur. |
| [[policies/file-upload-methodology_policy]] | file-upload-methodology_policy | Use when File Upload Methodology methodology is triggered and active testing may occur. |
| [[policies/fingerprintx-probe_policy]] | fingerprintx-probe_policy | Use when Fingerprintx Probe methodology is triggered and active testing may occur. |
| [[policies/firmware-analysis_policy]] | firmware-analysis_policy | Use when Firmware Analysis methodology is triggered and active testing may occur. |
| [[policies/fscan-scan_policy]] | fscan-scan_policy | Use when Fscan Scan methodology is triggered and active testing may occur. |
| [[policies/ftp-pentesting_policy]] | ftp-pentesting_policy | Use when Ftp Pentesting methodology is triggered and active testing may occur. |
| [[policies/gau-urlfetch_policy]] | gau-urlfetch_policy | Use when Gau Urlfetch methodology is triggered and active testing may occur. |
| [[policies/gcp-exploit_policy]] | gcp-exploit_policy | Use when Gcp Exploit methodology is triggered and active testing may occur. |
| [[policies/gcp-pentesting_policy]] | gcp-pentesting_policy | Use when Gcp Pentesting methodology is triggered and active testing may occur. |
| [[policies/gcp-workspace-pivot_policy]] | gcp-workspace-pivot_policy | Use when Gcp Workspace Pivot methodology is triggered and active testing may occur. |
| [[policies/githacker-git-leak_policy]] | githacker-git-leak_policy | Use when Githacker Git Leak methodology is triggered and active testing may occur. |
| [[policies/gogo-scan_policy]] | gogo-scan_policy | Use when Gogo Scan methodology is triggered and active testing may occur. |
| [[policies/graphql-methodology_policy]] | graphql-methodology_policy | Use when Graphql Methodology methodology is triggered and active testing may occur. |
| [[policies/http-host-header-attacks_policy]] | http-host-header-attacks_policy | Use when Http Host Header Attacks methodology is triggered and active testing may occur. |
| [[policies/http-smuggling-advanced_policy]] | http-smuggling-advanced_policy | Use when Http Smuggling Advanced methodology is triggered and active testing may occur. |
| [[policies/httpx-probe_policy]] | httpx-probe_policy | Use when Httpx Probe methodology is triggered and active testing may occur. |
| [[policies/huawei-pentesting_policy]] | huawei-pentesting_policy | Use when Huawei Pentesting methodology is triggered and active testing may occur. |
| [[policies/idor-methodology_policy]] | idor-methodology_policy | Use when Idor Methodology methodology is triggered and active testing may occur. |
| [[policies/imap-pentesting_policy]] | imap-pentesting_policy | Use when Imap Pentesting methodology is triggered and active testing may occur. |
| [[policies/information-disclosure-methodology_policy]] | information-disclosure-methodology_policy | Use when Information Disclosure Methodology methodology is triggered and active testing may occur. |
| [[policies/ioc-analysis_policy]] | ioc-analysis_policy | Use when Ioc Analysis methodology is triggered and active testing may occur. |
| [[policies/ios-exploiting_policy]] | ios-exploiting_policy | Use when Ios Exploiting methodology is triggered and active testing may occur. |
| [[policies/ios-pentesting_policy]] | ios-pentesting_policy | Use when Ios Pentesting methodology is triggered and active testing may occur. |
| [[policies/iox-proxy_policy]] | iox-proxy_policy | Use when Iox Proxy methodology is triggered and active testing may occur. |
| [[policies/java-deserialization-methodology_policy]] | java-deserialization-methodology_policy | Use when Java Deserialization Methodology methodology is triggered and active testing may occur. |
| [[policies/js-api-extract_policy]] | js-api-extract_policy | Use when Js Api Extract methodology is triggered and active testing may occur. |
| [[policies/judge-ctf_policy]] | judge-ctf_policy | Use when Judge CTF methodology is triggered and active testing may occur. |
| [[policies/judge-pentest_policy]] | judge-pentest_policy | Use when Judge Pentest methodology is triggered and active testing may occur. |
| [[policies/jwt-attack-methodology_policy]] | jwt-attack-methodology_policy | Use when Jwt Attack Methodology methodology is triggered and active testing may occur. |
| [[policies/k8s-container-escape_policy]] | k8s-container-escape_policy | Use when K8S Container Escape methodology is triggered and active testing may occur. |
| [[policies/k8s-ingress-nightmare_policy]] | k8s-ingress-nightmare_policy | Use when K8S Ingress Nightmare methodology is triggered and active testing may occur. |
| [[policies/k8s-istio-bypass_policy]] | k8s-istio-bypass_policy | Use when K8S Istio Bypass methodology is triggered and active testing may occur. |
| [[policies/k8s-network-recon_policy]] | k8s-network-recon_policy | Use when K8S Network Recon methodology is triggered and active testing may occur. |
| [[policies/k8s-sidecar-attack_policy]] | k8s-sidecar-attack_policy | Use when K8S Sidecar Attack methodology is triggered and active testing may occur. |
| [[policies/k8s-storage-exploit_policy]] | k8s-storage-exploit_policy | Use when K8S Storage Exploit methodology is triggered and active testing may occur. |
| [[policies/k8s-webhook-abuse_policy]] | k8s-webhook-abuse_policy | Use when K8S Webhook Abuse methodology is triggered and active testing may occur. |
| [[policies/k8spider_policy]] | k8spider_policy | Use when K8Spider methodology is triggered and active testing may occur. |
| [[policies/kafka-attack_policy]] | kafka-attack_policy | Use when Kafka Attack methodology is triggered and active testing may occur. |
| [[policies/katana-crawl_policy]] | katana-crawl_policy | Use when Katana Crawl methodology is triggered and active testing may occur. |
| [[policies/kerberos-pentesting_policy]] | kerberos-pentesting_policy | Use when Kerberos Pentesting methodology is triggered and active testing may occur. |
| [[policies/ksubdomain-brute_policy]] | ksubdomain-brute_policy | Use when Ksubdomain Brute methodology is triggered and active testing may occur. |
| [[policies/ldap-injection_policy]] | ldap-injection_policy | Use when Ldap Injection methodology is triggered and active testing may occur. |
| [[policies/ldap-pentesting_policy]] | ldap-pentesting_policy | Use when Ldap Pentesting methodology is triggered and active testing may occur. |
| [[policies/lfi-rfi-methodology_policy]] | lfi-rfi-methodology_policy | Use when Lfi Rfi Methodology methodology is triggered and active testing may occur. |
| [[policies/local-resources_policy]] | local-resources_policy | Use when Local Resources methodology is triggered and active testing may occur. |
| [[policies/log-evasion_policy]] | log-evasion_policy | Use when Log Evasion methodology is triggered and active testing may occur. |
| [[policies/mapcidr-cidr_policy]] | mapcidr-cidr_policy | Use when Mapcidr Cidr methodology is triggered and active testing may occur. |
| [[policies/mass-assignment_policy]] | mass-assignment_policy | Use when Mass Assignment methodology is triggered and active testing may occur. |
| [[policies/masscan-scan_policy]] | masscan-scan_policy | Use when Masscan Scan methodology is triggered and active testing may occur. |
| [[policies/memcache-pentesting_policy]] | memcache-pentesting_policy | Use when Memcache Pentesting methodology is triggered and active testing may occur. |
| [[policies/memory-forensics-evasion_policy]] | memory-forensics-evasion_policy | Use when Memory Forensics Evasion methodology is triggered and active testing may occur. |
| [[policies/mobile-backend_policy]] | mobile-backend_policy | Use when Mobile Backend methodology is triggered and active testing may occur. |
| [[policies/mongodb-attack_policy]] | mongodb-attack_policy | Use when Mongodb Attack methodology is triggered and active testing may occur. |
| [[policies/mssql-pentesting_policy]] | mssql-pentesting_policy | Use when Mssql Pentesting methodology is triggered and active testing may occur. |
| [[policies/mysql-pentesting_policy]] | mysql-pentesting_policy | Use when Mysql Pentesting methodology is triggered and active testing may occur. |
| [[policies/naabu-portscan_policy]] | naabu-portscan_policy | Use when Naabu Portscan methodology is triggered and active testing may occur. |
| [[policies/nerva-scan_policy]] | nerva-scan_policy | Use when Nerva Scan methodology is triggered and active testing may occur. |
| [[policies/nikto-scan_policy]] | nikto-scan_policy | Use when Nikto Scan methodology is triggered and active testing may occur. |
| [[policies/nmap-scan_policy]] | nmap-scan_policy | Use when Nmap Scan methodology is triggered and active testing may occur. |
| [[policies/nosql-injection_policy]] | nosql-injection_policy | Use when Nosql Injection methodology is triggered and active testing may occur. |
| [[policies/nuclei-scan_policy]] | nuclei-scan_policy | Use when Nuclei Scan methodology is triggered and active testing may occur. |
| [[policies/oauth-sso-attack_policy]] | oauth-sso-attack_policy | Use when Oauth Sso Attack methodology is triggered and active testing may occur. |
| [[policies/open-redirect_policy]] | open-redirect_policy | Use when Open Redirect methodology is triggered and active testing may occur. |
| [[policies/oss-bucket-exploit_policy]] | oss-bucket-exploit_policy | Use when Oss Bucket Exploit methodology is triggered and active testing may occur. |
| [[policies/ot-ics-attack_policy]] | ot-ics-attack_policy | Use when Ot Ics Attack methodology is triggered and active testing may occur. |
| [[policies/padbuster-padding-oracle_policy]] | padbuster-padding-oracle_policy | Use when Padbuster Padding Oracle methodology is triggered and active testing may occur. |
| [[policies/passive-recon_policy]] | passive-recon_policy | Use when Passive Recon methodology is triggered and active testing may occur. |
| [[policies/php-bypass_policy]] | php-bypass_policy | Use when Php Bypass methodology is triggered and active testing may occur. |
| [[policies/php-type-juggling_policy]] | php-type-juggling_policy | Use when Php Type Juggling methodology is triggered and active testing may occur. |
| [[policies/pop-pentesting_policy]] | pop-pentesting_policy | Use when Pop Pentesting methodology is triggered and active testing may occur. |
| [[policies/postgresql-attack_policy]] | postgresql-attack_policy | Use when Postgresql Attack methodology is triggered and active testing may occur. |
| [[policies/postgresql-pentesting_policy]] | postgresql-pentesting_policy | Use when Postgresql Pentesting methodology is triggered and active testing may occur. |
| [[policies/privilege-escalation-web_policy]] | privilege-escalation-web_policy | Use when Privilege Escalation Web methodology is triggered and active testing may occur. |
| [[policies/prompt-injection_policy]] | prompt-injection_policy | Use when Prompt Injection methodology is triggered and active testing may occur. |
| [[policies/prompt-jailbreak_policy]] | prompt-jailbreak_policy | Use when Prompt Jailbreak methodology is triggered and active testing may occur. |
| [[policies/prompt-leak_policy]] | prompt-leak_policy | Use when Prompt Leak methodology is triggered and active testing may occur. |
| [[policies/prototype-pollution-exploit_policy]] | prototype-pollution-exploit_policy | Use when Prototype Pollution Exploit methodology is triggered and active testing may occur. |
| [[policies/python-prototype-pollution_policy]] | python-prototype-pollution_policy | Use when Python Prototype Pollution methodology is triggered and active testing may occur. |
| [[policies/python-web-debug_policy]] | python-web-debug_policy | Use when Python Web Debug methodology is triggered and active testing may occur. |
| [[policies/race-condition-exploit_policy]] | race-condition-exploit_policy | Use when Race Condition Exploit methodology is triggered and active testing may occur. |
| [[policies/rdp-pentesting_policy]] | rdp-pentesting_policy | Use when Rdp Pentesting methodology is triggered and active testing may occur. |
| [[policies/recon-full_policy]] | recon-full_policy | Use when Recon Full methodology is triggered and active testing may occur. |
| [[policies/red-team-assessment_policy]] | red-team-assessment_policy | Use when Red Team Assessment methodology is triggered and active testing may occur. |
| [[policies/redis-attack_policy]] | redis-attack_policy | Use when Redis Attack methodology is triggered and active testing may occur. |
| [[policies/redis-pentesting_policy]] | redis-pentesting_policy | Use when Redis Pentesting methodology is triggered and active testing may occur. |
| [[policies/report-generate_policy]] | report-generate_policy | Use when Report Generate methodology is triggered and active testing may occur. |
| [[policies/serverless-attack_policy]] | serverless-attack_policy | Use when Serverless Attack methodology is triggered and active testing may occur. |
| [[policies/session-fixation_policy]] | session-fixation_policy | Use when Session Fixation methodology is triggered and active testing may occur. |
| [[policies/smb-pentesting_policy]] | smb-pentesting_policy | Use when Smb Pentesting methodology is triggered and active testing may occur. |
| [[policies/smtp-pentesting_policy]] | smtp-pentesting_policy | Use when Smtp Pentesting methodology is triggered and active testing may occur. |
| [[policies/snmp-pentesting_policy]] | snmp-pentesting_policy | Use when Snmp Pentesting methodology is triggered and active testing may occur. |
| [[policies/social-engineering_policy]] | social-engineering_policy | Use when Social Engineering methodology is triggered and active testing may occur. |
| [[policies/sql-injection-methodology_policy]] | sql-injection-methodology_policy | Use when Sql Injection Methodology methodology is triggered and active testing may occur. |
| [[policies/sqlmap-advanced_policy]] | sqlmap-advanced_policy | Use when Sqlmap Advanced methodology is triggered and active testing may occur. |
| [[policies/sqlserver-attack_policy]] | sqlserver-attack_policy | Use when Sqlserver Attack methodology is triggered and active testing may occur. |
| [[policies/ssh-pentesting_policy]] | ssh-pentesting_policy | Use when Ssh Pentesting methodology is triggered and active testing may occur. |
| [[policies/ssrf-methodology_policy]] | ssrf-methodology_policy | Use when Ssrf Methodology methodology is triggered and active testing may occur. |
| [[policies/ssti-methodology_policy]] | ssti-methodology_policy | Use when Ssti Methodology methodology is triggered and active testing may occur. |
| [[policies/subdomain-deep_policy]] | subdomain-deep_policy | Use when Subdomain Deep methodology is triggered and active testing may occur. |
| [[policies/subdomain-takeover_policy]] | subdomain-takeover_policy | Use when Subdomain Takeover methodology is triggered and active testing may occur. |
| [[policies/subfinder-recon_policy]] | subfinder-recon_policy | Use when Subfinder Recon methodology is triggered and active testing may occur. |
| [[policies/supply-chain-attack_policy]] | supply-chain-attack_policy | Use when Supply Chain Attack methodology is triggered and active testing may occur. |
| [[policies/supply-chain-audit_policy]] | supply-chain-audit_policy | Use when Supply Chain Audit methodology is triggered and active testing may occur. |
| [[policies/target-profiling_policy]] | target-profiling_policy | Use when Target Profiling methodology is triggered and active testing may occur. |
| [[policies/tencent-pentesting_policy]] | tencent-pentesting_policy | Use when Tencent Pentesting methodology is triggered and active testing may occur. |
| [[policies/threat-hunting-evasion_policy]] | threat-hunting-evasion_policy | Use when Threat Hunting Evasion methodology is triggered and active testing may occur. |
| [[policies/tlsx-probe_policy]] | tlsx-probe_policy | Use when Tlsx Probe methodology is triggered and active testing may occur. |
| [[policies/uncover-search_policy]] | uncover-search_policy | Use when Uncover Search methodology is triggered and active testing may occur. |
| [[policies/vnc-pentesting_policy]] | vnc-pentesting_policy | Use when Vnc Pentesting methodology is triggered and active testing may occur. |
| [[policies/voip-pentesting_policy]] | voip-pentesting_policy | Use when Voip Pentesting methodology is triggered and active testing may occur. |
| [[policies/waf-bypass-methodology_policy]] | waf-bypass-methodology_policy | Use when payload is blocked by WAF (403/406) and bypass is needed. |
| [[policies/web-vuln-scan_policy]] | web-vuln-scan_policy | Use when Web Vuln Scan methodology is triggered and active testing may occur. |
| [[policies/webshell-deploy_policy]] | webshell-deploy_policy | Use when Webshell Deploy methodology is triggered and active testing may occur. |
| [[policies/webshell-management_policy]] | webshell-management_policy | Use when Webshell Management methodology is triggered and active testing may occur. |
| [[policies/websocket-attack_policy]] | websocket-attack_policy | Use when Websocket Attack methodology is triggered and active testing may occur. |
| [[policies/winrm-pentesting_policy]] | winrm-pentesting_policy | Use when Winrm Pentesting methodology is triggered and active testing may occur. |
| [[policies/xray-scan_policy]] | xray-scan_policy | Use when Xray Scan methodology is triggered and active testing may occur. |
| [[policies/xslt-injection_policy]] | xslt-injection_policy | Use when Xslt Injection methodology is triggered and active testing may occur. |
| [[policies/xss-methodology_policy]] | xss-methodology_policy | Use when Xss Methodology methodology is triggered and active testing may occur. |
| [[policies/xxe-injection-methodology_policy]] | xxe-injection-methodology_policy | Use when Xxe Injection Methodology methodology is triggered and active testing may occur. |
