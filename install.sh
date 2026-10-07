#!/bin/sh
set -eu

repo=${PACKTIDE_REPOSITORY:-muwenyan521/packtide}
version=${PACKTIDE_VERSION:-0.1.2}
lang=${PACKTIDE_UI_LANG:-${LC_ALL:-${LC_MESSAGES:-${LANG:-en}}}}
case "$lang" in zh*|ZH*) lang=zh ;; *) lang=en ;; esac

say() { printf '%s\n' "$*"; }
msg() {
    if [ "$lang" = zh ]; then
        case "$1" in
            detect) say "检测到发行版：$2（$3 架构，$4 运行时）" ;;
            deps) say "正在补全运行依赖：$*" ;;
            replace) say "检测到已有命令：$2。是否备份并替换？[y/N]" ;;
            install_done) say "安装完成：$2" ;;
            skip) say "保留已有命令：$2" ;;
            unsupported) say "无法自动安装依赖，请先安装 fzf 和权限工具后重试。" ;;
            *) say "$*" ;;
        esac
    else
        case "$1" in
            detect) say "Detected: $2 ($3, $4 runtime)" ;;
            deps) say "Installing runtime dependencies: $*" ;;
            replace) say "Existing command found: $2. Back up and replace it? [y/N]" ;;
            install_done) say "Installed: $2" ;;
            skip) say "Keeping existing command: $2" ;;
            unsupported) say "Install fzf and a privilege helper, then run this script again." ;;
            *) say "$*" ;;
        esac
    fi
}

need_cmd() { command -v "$1" >/dev/null 2>&1; }
run_privileged() {
    if [ -n "$as_root" ]; then
        "$as_root" "$@"
    else
        "$@"
    fi
}
if ! need_cmd curl || ! need_cmd install; then
    say "curl and install are required" >&2
    exit 1
fi

arch=$(uname -m)
case "$arch" in x86_64|amd64) artifact_arch=x86_64 ;; *) say "Unsupported architecture: $arch" >&2; exit 1 ;; esac
if command -v ldd >/dev/null 2>&1 && ldd --version 2>&1 | grep -qi musl; then
    libc=musl
else
    libc=gnu
fi

distro=unknown
if [ -r /etc/os-release ]; then
    . /etc/os-release
    distro=${ID:-unknown}
fi
msg detect "$distro" "$artifact_arch" "$libc"

as_root=""
if [ "$(id -u)" -eq 0 ]; then
    as_root=
    prefix=/opt/packtide
    bindir=/usr/local/bin
else
    if need_cmd sudo; then as_root=sudo; elif need_cmd doas; then as_root=doas; fi
    prefix=${XDG_DATA_HOME:-$HOME/.local/share}/packtide
    bindir=${XDG_BIN_HOME:-$HOME/.local/bin}
fi

if ! need_cmd fzf || { [ "$(id -u)" -eq 0 ] && ! need_cmd sudo && ! need_cmd doas; }; then
    if [ "$(id -u)" -ne 0 ] && ! need_cmd fzf && [ -z "$as_root" ]; then
        msg unsupported
        exit 1
    fi
    if [ "$distro" = arch ]; then
        msg deps pacman fzf sudo; run_privileged pacman -Sy --needed --noconfirm fzf sudo
    elif [ "$distro" = debian ] || [ "$distro" = ubuntu ]; then
        msg deps apt-get fzf sudo; run_privileged apt-get update; run_privileged apt-get install -y fzf sudo
    elif [ "$distro" = fedora ] || [ "$distro" = rocky ] || [ "$distro" = rhel ]; then
        msg deps dnf fzf sudo; run_privileged dnf install -y fzf sudo
    elif case "$distro" in opensuse*|suse*) true ;; *) false ;; esac; then
        msg deps zypper fzf sudo; run_privileged zypper --non-interactive install fzf sudo
    elif [ "$distro" = alpine ]; then
        msg deps apk fzf doas; run_privileged apk add fzf doas
    elif [ "$distro" = void ]; then
        msg deps xbps-install fzf sudo; run_privileged xbps-install -Sy fzf sudo
    else
        msg unsupported
    fi
fi

archive="packtide-${version}-${artifact_arch}-unknown-linux-${libc}.tar.gz"
base=${PACKTIDE_BASE_URL:-"https://github.com/${repo}/releases/download/v${version}"}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM
curl --fail --location --silent --show-error "$base/$archive" -o "$tmp/$archive"
tar -xzf "$tmp/$archive" -C "$tmp"
payload=$(find "$tmp" -mindepth 1 -maxdepth 1 -type d | head -n 1)

mkdir -p "$prefix" "$bindir"
install -m 0755 "$payload/packtide" "$prefix/packtide"
install -m 0755 "$payload/systide" "$prefix/systide"

for command in pac pacr pacrrr sysup; do
    if path=$(command -v "$command" 2>/dev/null); then
        msg replace "$command"
        if [ "$lang" = zh ]; then read -r answer || answer=; else read -r answer || answer=; fi
        case "$answer" in y|Y|yes|YES|是)
            backup="$path.packtide-backup.$(date +%Y%m%d%H%M%S)"
            if [ -w "$path" ] || [ -w "$(dirname "$path")" ]; then
                mv "$path" "$backup"
            elif [ -n "$as_root" ]; then
                "$as_root" mv "$path" "$backup"
            else
                msg skip "$command"
                continue
            fi
            target=$prefix/packtide
            case "$command" in sysup) target=$prefix/systide ;; esac
            if [ -w "$(dirname "$path")" ]; then ln -s "$target" "$path"; else "$as_root" ln -s "$target" "$path"; fi
            ;;
        *) msg skip "$command"; continue ;;
        esac
    fi
done

ln -sfn "$prefix/packtide" "$bindir/packtide"
ln -sfn "$prefix/systide" "$bindir/systide"
ln -sfn "$prefix/packtide" "$bindir/ptd"
ln -sfn "$prefix/systide" "$bindir/suu"
profile=${XDG_CONFIG_HOME:-$HOME/.config}/packtide/profile
mkdir -p "$(dirname "$profile")"
printf '%s\n' "export PATH=\"$bindir:\$PATH\"" > "$profile"
for shell_profile in "$HOME/.profile" "$HOME/.bashrc"; do
    touch "$shell_profile"
    grep -Fqx ". \"$profile\"" "$shell_profile" 2>/dev/null || printf '%s\n' ". \"$profile\"" >> "$shell_profile"
done
msg install_done "$prefix (packtide/ptd and systide/suu)"
