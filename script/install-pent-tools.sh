#!/usr/bin/env bash
# 一键安装 Kairos Platform 渗透测试工具集。
# 用法:
#   root:    sudo bash script/install-pent-tools.sh
#   非 root: bash script/install-pent-tools.sh   （跳过 apt，装入用户目录）
#
# 说明:
#   - apt 分组采用「尽力而为」策略（|| true）：个别包在非 Kali 源找不到时
#     不会中断，改由后面的 go/pip/cargo/gem 分组兜底安装。
#   - 受控工具目录（$PENTA_TOOL_DIR，默认 ~/.kairos/tools/bin）是发现与沙箱
#     挂载的唯一锚点：pent-agent 启动时会扫描 PATH 并把工具软链至此，无需
#     依赖 root 固定路径或 agent 的运行时 PATH。
#   - metasploit-framework 体积较大，单独分组，失败不阻断其余工具。

set -uo pipefail

TOOL_DIR="${PENTA_TOOL_DIR:-$HOME/.kairos/tools/bin}"
mkdir -p "$TOOL_DIR"

if [[ $EUID -eq 0 ]]; then
    echo "以 root 运行：启用 apt 分组。"
else
    echo "非 root 运行：跳过 apt 分组，仅安装 go/pip/cargo/gem/git 工具。"
fi

if [[ $EUID -eq 0 ]]; then
    echo "==> [1/8] 基础工具链"
    apt-get update
    apt-get install -y --no-install-recommends bubblewrap
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
else
    echo "跳过 [1/8] [2/8] apt 分组（需要 root）。"
fi

echo "==> [3/8] Go 工具"
if [[ $EUID -eq 0 && -z "${PENTA_TOOL_DIR:-}" ]]; then
    export GOBIN=/usr/local/bin
else
    export GOBIN="$TOOL_DIR"
fi
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
PIP_FLAGS=(--no-input)
if [[ $EUID -eq 0 ]]; then
    PIP_FLAGS+=(--break-system-packages)
else
    PIP_FLAGS+=(--user)
fi
for pkg in $PIP_TOOLS; do
    pip3 install "${PIP_FLAGS[@]}" "$pkg" || echo "  [跳过] $pkg"
done

echo "==> [5/8] cargo 工具"
if [[ $EUID -ne 0 ]]; then
    export CARGO_INSTALL_ROOT="$(dirname "$TOOL_DIR")"
fi
cargo install feroxbuster rustscan pwninit

echo "==> [6/8] gem 工具"
gem install zsteg one_gadget

echo "==> [7/8] git 安装工具"
if [[ $EUID -eq 0 ]]; then
    GIT_ROOT=/opt
    LINK_DIR=/usr/local/bin
else
    GIT_ROOT="$HOME/.kairos/tools/src"
    LINK_DIR="$TOOL_DIR"
fi
mkdir -p "$GIT_ROOT" "$LINK_DIR"

clone_or_pull() {
    local repo=$1 dir=$2
    if [[ -d "$dir/.git" ]]; then
        git -C "$dir" pull --ff-only || true
    else
        rm -rf "$dir"
        git clone --depth 1 "$repo" "$dir" || return 1
    fi
}

clone_or_pull https://github.com/sqlmapproject/sqlmap.git "$GIT_ROOT/sqlmap"
ln -sf "$GIT_ROOT/sqlmap/sqlmap.py" "$LINK_DIR/sqlmap"

clone_or_pull https://github.com/commixproject/commix.git "$GIT_ROOT/commix"
ln -sf "$GIT_ROOT/commix/commix.py" "$LINK_DIR/commix"

clone_or_pull https://github.com/devanshbatham/ParamSpider "$GIT_ROOT/ParamSpider"
if [[ -f "$GIT_ROOT/ParamSpider/requirements.txt" ]]; then
    pip3 install "${PIP_FLAGS[@]}" -r "$GIT_ROOT/ParamSpider/requirements.txt" || true
fi
ln -sf "$GIT_ROOT/ParamSpider/paramspider.py" "$LINK_DIR/paramspider"

# sslcaudit（PyPI/apt 均无，需从源码编译，失败不阻断）
clone_or_pull https://github.com/grwl/sslcaudit "$GIT_ROOT/sslcaudit"
if [[ -f "$GIT_ROOT/sslcaudit/Makefile" ]]; then
    make -C "$GIT_ROOT/sslcaudit" || true
    ln -sf "$GIT_ROOT/sslcaudit/sslcaudit" "$LINK_DIR/sslcaudit" 2>/dev/null || true
fi

if [[ $EUID -eq 0 ]]; then
    echo "==> [8/8] metasploit-framework（体积大，失败不阻断）"
    apt-get install -y metasploit-framework || true
else
    echo "跳过 metasploit-framework（需要 root）。"
fi

echo "==> 安装完成。可运行下方命令验证："
echo "    bwrap --version"
echo "    bash script/verify-tools.sh"
echo "受控工具目录: $TOOL_DIR（pent-agent 启动时会把已安装工具软链至此并挂载进沙箱）"
