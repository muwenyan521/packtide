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
            detected_manager) say "当前发行版未原生支持，将使用检测到的包管理器：$2" ;;
            unsupported_distro) say "暂不支持自动安装到此发行版：$2。请查看 README 中的支持列表。" ;;
            glibc_too_old) say "当前 glibc 为 $2；GNU 发布包要求 glibc 2.36 或更高版本。" ;;
            fzf_too_old) say "fzf 版本过旧：需要 0.74.0 或更高版本。" ;;
            deps) say "正在补全运行依赖：$*" ;;
            replace) say "检测到已有命令：$2。是否备份并替换？[y/N]" ;;
            install_done) say "安装完成：$2" ;;
            shell_path) say "请重新打开当前 shell，或执行：export PATH=\"$2:\$PATH\"" ;;
            fish_path) say "请重新打开 fish，或在当前 fish 中执行：fish_add_path -g -m $2" ;;
            skip) say "保留已有命令：$2" ;;
            unsupported) say "无法自动安装依赖，请先安装 fzf 和权限工具后重试。" ;;
            *) say "$*" ;;
        esac
    else
        case "$1" in
            detect) say "Detected: $2 ($3, $4 runtime)" ;;
            detected_manager) say "This distribution is not natively supported; using detected package manager: $2" ;;
            unsupported_distro) say "Automatic installation is not supported on $2. See the supported distributions in README." ;;
            glibc_too_old) say "glibc $2 detected; the GNU release requires glibc 2.36 or newer." ;;
            fzf_too_old) say "fzf is too old; version 0.74.0 or newer is required." ;;
            deps) say "Installing runtime dependencies: $*" ;;
            replace) say "Existing command found: $2. Back up and replace it? [y/N]" ;;
            install_done) say "Installed: $2" ;;
            shell_path) say "Open a new shell, or run this in the current shell: export PATH=\"$2:\$PATH\"" ;;
            fish_path) say "Open a new fish shell, or run this in the current fish session: fish_add_path -g -m $2" ;;
            skip) say "Keeping existing command: $2" ;;
            unsupported) say "Install fzf and a privilege helper, then run this script again." ;;
            *) say "$*" ;;
        esac
    fi
}

need_cmd() { command -v "$1" >/dev/null 2>&1; }
version_at_least() {
    awk -v actual="$1" -v required="$2" 'BEGIN {
        split(actual, a, "."); split(required, r, ".");
        for (i = 1; i <= 3; i++) {
            av = a[i] + 0; rv = r[i] + 0;
            if (av > rv) exit 0;
            if (av < rv) exit 1;
        }
        exit 0;
    }'
}
run_privileged() {
    if [ -n "$as_root" ]; then
        "$as_root" "$@"
    else
        "$@"
    fi
}
read_answer() {
    answer=
    if [ -r /dev/tty ]; then
        IFS= read -r answer </dev/tty || answer=
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
elif command -v ldd >/dev/null 2>&1 && ldd --version 2>&1 | grep -qi 'GNU libc'; then
    libc=gnu
else
    say "Cannot identify the system C library (glibc or musl)." >&2
    exit 1
fi

distro=unknown
os_release_file=${PACKTIDE_OS_RELEASE_FILE:-/etc/os-release}
if [ -r "$os_release_file" ]; then
    . "$os_release_file"
    distro=${ID:-unknown}
fi
msg detect "$distro" "$artifact_arch" "$libc"

native_command=
native_family=
native_supported=1
case "$distro" in
    arch|cachyos|manjaro) native_command=pacman; native_family=arch ;;
    debian|ubuntu) native_command=apt-get; native_family=apt ;;
    fedora|rocky|rhel) native_family=dnf ;;
    opensuse*|suse*) native_command=zypper; native_family=zypper ;;
    alpine) native_command=apk; native_family=apk ;;
    void) native_command=xbps-install; native_family=xbps ;;
    *) native_supported=0 ;;
esac

if [ "$native_supported" -eq 1 ] && [ "$native_family" = dnf ]; then
    if need_cmd dnf5; then native_command=dnf5; else native_command=dnf; fi
fi

if [ "$native_supported" -eq 0 ]; then
    for candidate in pacman apt-get dnf5 dnf zypper apk xbps-install; do
        if need_cmd "$candidate"; then
            native_command=$candidate
            case "$candidate" in
                pacman) native_family=arch ;;
                apt-get) native_family=apt ;;
                dnf5|dnf) native_family=dnf ;;
                zypper) native_family=zypper ;;
                apk) native_family=apk ;;
                xbps-install) native_family=xbps ;;
            esac
            break
        fi
    done
    if [ -z "$native_command" ]; then
        msg unsupported_distro "$distro" >&2
        exit 1
    fi
    msg detected_manager "$native_command"
fi

if [ "$libc" = gnu ]; then
    glibc_version=$(getconf GNU_LIBC_VERSION 2>/dev/null | awk '{print $2}')
    if [ -z "$glibc_version" ] || ! version_at_least "$glibc_version" 2.36; then
        msg glibc_too_old "${glibc_version:-unknown}" >&2
        exit 1
    fi
fi

if ! need_cmd "$native_command"; then
    say "Required native package manager not found: $native_command" >&2
    exit 1
fi

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

fzf_ok=0
if need_cmd fzf; then
    fzf_version=$(fzf --version 2>/dev/null | awk 'NR == 1 { print $1 }')
    if version_at_least "$fzf_version" 0.74.0; then fzf_ok=1; fi
fi
if [ "$fzf_ok" -ne 1 ]; then
    if need_cmd fzf; then msg fzf_too_old; fi
fi

if [ "$fzf_ok" -ne 1 ] || { [ "$(id -u)" -eq 0 ] && ! need_cmd sudo && ! need_cmd doas; }; then
    if [ "$(id -u)" -ne 0 ] && ! need_cmd fzf && [ -z "$as_root" ]; then
        msg unsupported
        exit 1
    fi
    if [ "$native_family" = arch ]; then
        msg deps pacman fzf sudo; run_privileged pacman -Sy --needed --noconfirm fzf sudo
    elif [ "$native_family" = apt ]; then
        msg deps apt-get fzf sudo; run_privileged apt-get update; run_privileged apt-get install -y fzf sudo
    elif [ "$native_family" = dnf ]; then
        msg deps "$native_command" fzf sudo; run_privileged "$native_command" install -y fzf sudo
    elif [ "$native_family" = zypper ]; then
        msg deps zypper fzf sudo; run_privileged zypper --non-interactive install fzf sudo
    elif [ "$native_family" = apk ]; then
        msg deps apk fzf doas; run_privileged apk add fzf doas
    elif [ "$native_family" = xbps ]; then
        msg deps xbps-install fzf sudo; run_privileged xbps-install -Sy fzf sudo
    fi
fi

if ! need_cmd fzf; then
    say "fzf installation did not provide a usable command." >&2
    exit 1
fi
fzf_version=$(fzf --version 2>/dev/null | awk 'NR == 1 { print $1 }')
if ! version_at_least "$fzf_version" 0.74.0; then
    msg fzf_too_old >&2
    exit 1
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
        read_answer
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

install -m 0755 "$prefix/packtide" "$bindir/packtide"
install -m 0755 "$prefix/systide" "$bindir/systide"
ln -sfn "$prefix/packtide" "$bindir/ptd"
ln -sfn "$prefix/systide" "$bindir/suu"
for command in packtide ptd systide suu; do
    if [ ! -x "$bindir/$command" ]; then
        say "Failed to create command link: $bindir/$command" >&2
        exit 1
    fi
done
profile=${XDG_CONFIG_HOME:-$HOME/.config}/packtide/profile
mkdir -p "$(dirname "$profile")"
printf '%s\n' "export PATH=\"$bindir:\$PATH\"" > "$profile"
for shell_profile in "$HOME/.profile" "$HOME/.bashrc"; do
    touch "$shell_profile"
    grep -Fqx ". \"$profile\"" "$shell_profile" 2>/dev/null || printf '%s\n' ". \"$profile\"" >> "$shell_profile"
done
fish_profile=${XDG_CONFIG_HOME:-$HOME/.config}/fish/conf.d/packtide.fish
mkdir -p "$(dirname "$fish_profile")"
printf '%s\n' "fish_add_path -g -m '$bindir'" > "$fish_profile"
msg install_done "$prefix (packtide/ptd and systide/suu)"
case "${SHELL##*/}" in
    fish) msg fish_path "$bindir" ;;
    bash|zsh|ksh) msg shell_path "$bindir" ;;
esac
