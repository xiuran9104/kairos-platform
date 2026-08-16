#!/usr/bin/env bash
# 一键安装 Kairos Platform 渗透测试工具集（需 root 权限）。
# 用法: sudo bash script/install-pent-tools.sh
#
# 说明:
#   - apt 分组采用「尽力而为」策略（|| true）：个别包在非 Kali 源找不到时
#     不会中断，改由后面的 go/pip/cargo/gem 分组兜底安装。
#   - Go 工具统一装到 /usr/local/bin（与 tool_lifecycle.rs 期望路径一致）。
#   - metasploit-framework 体积较大，单独分组，失败不阻断其余工具。

set -uo pipefail

if [[ $EUID -ne 0 ]]; then
    echo "请以 root 运行: sudo bash $0" >&2
    exit 1
fi

echo "==> [1/8] 基础工具链"
apt-get update
apt-get install -y --no-install-recommends \
    golang-go python3-pip cargo ruby-full git curl dnsutils openssh-client postgresql-client \
    build-essential pkg-config libssl-dev libgmp-dev \
    || true

echo "==> [2/8] apt 安全工具（找不到的由后续分组兜底）"
apt-get install -y --no-install-recommends \
    nmap nuclei subfinder amass masscan ffuf gobuster feroxbuster wfuzz dirsearch dirb \
    nikto whatweb wafw00f wpscan sqlmap xsser commix \
    testssl.sh sslyze httrack \
    dnsrecon dnsenum fierce dmitry theharvester \
    enum4linux smbmap smtp-user-enum swaks davtest nbtscan netdiscover \
    wapiti skipfish lbd \
    binwalk gdb binutils xxd checksec libimage-exiftool-perl foremost steghide \
    hashcat john hydra arp-scan smbclient dotdotpwn \
    radare2 responder trivy \
    || true

echo "==> [3/8] Go 工具（/usr/local/bin）"
export GOBIN=/usr/local/bin
go install -v github.com/projectdiscovery/nuclei/v3/cmd/nuclei@latest
go install -v github.com/zan8th/afrog/v2/cmd/afrog@latest
go install -v github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest
go install -v github.com/owasp-amass/amass/v4/...@master
go install -v github.com/projectdiscovery/dnsx/cmd/dnsx@latest
go install -v github.com/projectdiscovery/naabu/v2/cmd/naabu@latest
go install -v github.com/ffuf/ffuf/v2@latest
go install -v github.com/OJ/gobuster/v3@latest
go install -v github.com/hahwul/dalfox/v2@latest
go install -v github.com/hakluke/hakrawler@latest
go install -v github.com/tomnomnom/waybackurls@latest
go install -v github.com/aquasecurity/trivy/cmd/trivy@latest
go install -v github.com/tenable/terrascan/cmd/terrascan@latest
go install -v github.com/jaeles-project/jaeles@latest

echo "==> [4/8] pip 工具（逐个安装，失败跳过）"
PIP_TOOLS="dirsearch wfuzz wafw00f sslyze arjun dnsrecon fierce ROPgadget ropper volatility3 checkov kube-hunter scoutsuite prowler pacu hashpump impacket netexec bloodhound enum4linux-ng"
for pkg in $PIP_TOOLS; do
    pip3 install --break-system-packages --no-input "$pkg" || echo "  [跳过] $pkg"
done

echo "==> [5/8] cargo 工具"
cargo install feroxbuster rustscan pwninit

echo "==> [6/8] gem 工具"
gem install zsteg one_gadget

echo "==> [7/8] git 安装工具"
mkdir -p /opt

clone_or_pull() {
    local repo=$1 dir=$2
    if [[ -d "$dir/.git" ]]; then
        git -C "$dir" pull --ff-only || true
    else
        rm -rf "$dir"
        git clone --depth 1 "$repo" "$dir" || return 1
    fi
}

clone_or_pull https://github.com/sqlmapproject/sqlmap.git /opt/sqlmap
ln -sf /opt/sqlmap/sqlmap.py /usr/local/bin/sqlmap

clone_or_pull https://github.com/commixproject/commix.git /opt/commix
ln -sf /opt/commix/commix.py /usr/local/bin/commix

clone_or_pull https://github.com/devanshbatham/ParamSpider /opt/ParamSpider
if [[ -f /opt/ParamSpider/requirements.txt ]]; then
    pip3 install --break-system-packages -r /opt/ParamSpider/requirements.txt || true
fi
ln -sf /opt/ParamSpider/paramspider.py /usr/local/bin/paramspider

# sslcaudit（PyPI/apt 均无，需从源码编译，失败不阻断）
clone_or_pull https://github.com/grwl/sslcaudit /opt/sslcaudit
if [[ -f /opt/sslcaudit/Makefile ]]; then
    make -C /opt/sslcaudit || true
    ln -sf /opt/sslcaudit/sslcaudit /usr/local/bin/sslcaudit 2>/dev/null || true
fi

echo "==> [8/8] metasploit-framework（体积大，失败不阻断）"
apt-get install -y metasploit-framework || true

echo "==> 安装完成。可运行下方命令验证："
echo "    bash script/verify-tools.sh"
