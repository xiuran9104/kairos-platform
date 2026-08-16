#!/usr/bin/env bash
# 验证渗透测试工具的安装状态（无需 root）。
# 用法: bash script/verify-tools.sh
#
# 不再硬编码二进制绝对路径：工具会落在 /usr/bin、/usr/sbin、/usr/local/bin 或
# 用户工具链目录（~/.cargo/bin、~/go/bin、~/.local/bin、conda 等）。这里按
# 受控工具目录（$PENTA_TOOL_DIR，默认 ~/.kairos/tools/bin）与 PATH 联合发现。

set -uo pipefail

# 每个条目: "显示名:二进制名"（二进制名用于 PATH/受控目录发现）
TOOLS=(
    "nmap:nmap"
    "curl:curl"
    "nuclei:nuclei"
    "afrog:afrog"
    "dig:dig"
    "ssh:ssh"
    "psql:psql"
    "subfinder:subfinder"
    "amass:amass"
    "dnsx:dnsx"
    "masscan:masscan"
    "naabu:naabu"
    "ffuf:ffuf"
    "gobuster:gobuster"
    "feroxbuster:feroxbuster"
    "wfuzz:wfuzz"
    "dirsearch:dirsearch"
    "dirb:dirb"
    "nikto:nikto"
    "whatweb:whatweb"
    "wafw00f:wafw00f"
    "wpscan:wpscan"
    "sqlmap:sqlmap"
    "xsser:xsser"
    "dalfox:dalfox"
    "commix:commix"
    "testssl:testssl"
    "sslyze:sslyze"
    "qsslcaudit:qsslcaudit"
    "hakrawler:hakrawler"
    "httrack:httrack"
    "arjun:arjun"
    "dnsrecon:dnsrecon"
    "dnsenum:dnsenum"
    "fierce:fierce"
    "dmitry:dmitry"
    "theharvester:theharvester"
    "enum4linux:enum4linux"
    "smbmap:smbmap"
    "smtp-user-enum:smtp-user-enum"
    "swaks:swaks"
    "davtest:davtest"
    "nbtscan:nbtscan"
    "netdiscover:netdiscover"
    "wapiti:wapiti"
    "skipfish:skipfish"
    "lbd:lbd"
    "binwalk:binwalk"
    "gdb:gdb"
    "objdump:objdump"
    "strings:strings"
    "xxd:xxd"
    "checksec:checksec"
    "ropgadget:ROPgadget"
    "ropper:ropper"
    "exiftool:exiftool"
    "foremost:foremost"
    "steghide:steghide"
    "zsteg:zsteg"
    "volatility3:vol3"
    "hashcat:hashcat"
    "john:john"
    "hydra:hydra"
    "arp-scan:arp-scan"
    "rpcclient:rpcclient"
    "dotdotpwn:dotdotpwn"
    "rustscan:rustscan"
    "waybackurls:waybackurls"
    "paramspider:paramspider"
    "radare2:r2"
    "responder:responder"
    "msfvenom:msfvenom"
    "trivy:trivy"
    "checkov:checkov"
    "kube-hunter:kube-hunter"
    "scout-suite:scout"
    "terrascan:terrascan"
    "prowler:prowler"
    "pacu:pacu"
    "jaeles:jaeles"
    "pwninit:pwninit"
    "one-gadget:one_gadget"
    "hashpump:hashpump"
    "impacket:impacket-secretsdump"
    "netexec:netexec"
    "bloodhound:bloodhound-python"
    "enum4linux-ng:enum4linux-ng"
)

TOOL_DIR="${PENTA_TOOL_DIR:-$HOME/.kairos/tools/bin}"
# 扩展 PATH，覆盖受控目录与常见用户工具链目录
export PATH="$TOOL_DIR:$HOME/.cargo/bin:$HOME/go/bin:$HOME/.local/bin:$PATH"

missing=0
for entry in "${TOOLS[@]}"; do
    name="${entry%%:*}"
    bin="${entry#*:}"
    if command -v "$bin" >/dev/null 2>&1; then
        printf '  [已装] %s\n' "$name"
    else
        printf '  [缺失] %s\n' "$name"
        missing=$((missing + 1))
    fi
done

echo "----------------------------------------"
echo "合计: ${#TOOLS[@]} 个工具, 缺失 $missing 个"
