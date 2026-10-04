# packtide / systide

独立的 Linux 系统工具 workspace：

- `packtide`：面向多发行版的包管理 TUI/CLI。
- `systide`：面向多发行版的系统更新编排器。

本项目不依赖 `shorin-contrib`、`shorin-pac` 或其他特定生态。原项目仅作为交互风格和需求边界的参考。

## 第一轮范围

- `packtide` 默认行为等价于原 `pac`：安装 TUI；`packtide <query>` 预填查询。
- `packtide remove` 覆盖原 `pacr` 的 Pacman/AUR/Flatpak 卸载主流程。
- `packtide check-updates`、`mirror-update` 和 `downgrade` 覆盖原本地工具入口。
- `systide` 是唯一的系统更新编排入口，负责新闻、镜像、快照、keyring、包升级、Flatpak、GRUB 和 Waybar 收尾。
- `packtide sysup` 仅作为旧脚本兼容桥，将参数结构化转发给同目录或 PATH 中的 `systide`；它不再维护第二套系统更新逻辑。
- 第一轮保留 `fzf`，先保证原有键位、preview、颜色和页面行为；Ratatui 属于后续阶段。
- AUR AI 审查和 AI 残留清理按已确认范围暂不迁移。
- 旧入口的 `--no-ai` 仍被接受并作为兼容 no-op，不会启用或调用任何 AI 功能。

## 语言与资源

当前 UI 资源集中在两个 binary 各自的 `locales/` 目录：

- `crates/packtide/locales/en.txt`
- `crates/packtide/locales/zh.txt`
- `crates/systide/locales/en.txt`
- `crates/systide/locales/zh.txt`

`systide` 使用 `--ui-lang auto|zh|en`；`auto` 按 `LC_ALL`、`LC_MESSAGES`、`LANG`
选择中文或英文。`packtide` 使用 `PACKTIDE_UI_LANG=auto|zh|en`，`auto` 同样读取
locale 环境变量。新增语言只需增加对应资源文件并接入 loader，不改事务参数、包名
解析或退出码。

资源文件是构建时嵌入的本地文本，不需要网络，也不会把用户可见文案混进命令 argv。

## 支持矩阵与行为边界

`system-tools-core` 为每个后端保留 typed identity、scope 和 capability。后端分为
发行版原生后端与可选后端；每个后端的 scope 单独定义。检测不到可选命令时会跳过，
缺少原生工具或请求不支持的能力时会返回明确错误，不会把另一个后端当作替代品。

| 后端 | 包类型与 scope | `systide` 更新 | `packtide` 交互路径 | 矩阵验证 |
| --- | --- | --- | --- | --- |
| Pacman | system | `pacman -Su`（特权）及 keyring | install/remove/check-updates/downgrade | Arch smoke（宿主） |
| APT | system | `apt-get upgrade -y`（特权） | core contract；无 Arch picker | Debian、Ubuntu container |
| DNF5 | system | `dnf upgrade -y`（特权） | core contract；无 Arch picker | Fedora container |
| DNF4 | system | `dnf upgrade -y`（特权） | core contract；无 Arch picker | Rocky、Alma container |
| Zypper | system | `zypper update -y`（特权） | core contract；无 Arch picker | openSUSE container |
| APK | system | `apk upgrade`（特权） | core contract；无 Arch picker | Alpine container |
| XBPS | system | `xbps-install -Su`（特权） | core contract；无 Arch picker | Void container |
| Paru / Yay | AUR user | 不由 `systide` 自动执行 | Arch AUR install/remove/check-updates/downgrade | fixture/typed contract |
| Flatpak | user 或 system | `flatpak update -y`（存在时） | Flatpak install/remove/update | fixture/typed contract |
| Snap | system | `snap refresh`（存在时） | 只提供 typed backend；不在 Arch picker 中 | Ubuntu QEMU VM（见下文） |
| Brew | Linuxbrew profile | `brew upgrade`（存在时） | 只提供 typed backend；不在 Arch picker 中 | pinned Homebrew container |
| Nix | profile | `nix profile upgrade .*`（存在时） | 只提供 typed backend；不操作 system store | pinned Nix container |

`systide` 先更新已检测到的原生后端，再按 PATH 中实际存在的命令尝试 Flatpak、Snap、
Brew、Nix；可选步骤的失败会保留在结果中并使最终退出码为非零。AUR helper 不属于
`systide` 的可选更新列表。`packtide` 的 `mirror-update`、`downgrade`、`sysup` 仍是
Pacman/Arch 兼容入口，不能把它们当作 APT、DNF、Zypper、APK 或 XBPS 的通用命令。

读取、搜索、installed、details 和 updates 是后端 capability 的独立边界；不支持的
操作返回 typed `UnsupportedCapability`。这意味着“能被矩阵探测”不等于“所有
`packtide` picker 命令都可用”。Snap 只接受 system scope 的商店包，不接受本地
`.snap`/`--dangerous` 安装源；Brew 在 Linux 仅支持 formula，不支持 cask；Nix 只
操作 profile，不操作 system store。除 Pacman/AUR/Flatpak 已接入 `packtide` 交互
路径外，其余后端的读取或写入能力必须由调用方按 capability 检查，不能把后端名
当作功能承诺。

完整的后端、scope、命令和验证 lane 见 [`docs/package-manager-support.md`](docs/package-manager-support.md)。

## 当前状态

当前是 Rust workspace，包含两个 release binary：`packtide` 和 `systide`。

共享边界位于 `crates/system-tools-core`，提供结构化命令执行、特权环境清理、可执行文件解析和原子 TTL 缓存。`packtide` 的 CLI、包源、UI/preview、事务和系统更新入口已按职责拆到 `cli.rs`、`sources.rs`、`ui.rs`、`transaction.rs`、`commands.rs`；`systide` 的 CLI、消息、新闻、UI 和更新操作分别位于对应模块。

基础运行依赖：

- `fzf`
- 当前发行版对应的原生包工具（见支持矩阵）
- `sudo`（执行 system scope 事务时）

按功能可选运行依赖：

- `paru` 或 `yay`（Arch AUR 路径）
- `flatpak`、`snap`、`brew`、`nix`（对应可选后端；不存在时跳过）
- `reflector`（`mirror-update`）、`quicksave`/`findmnt`（snapshot）
- `grub-mkconfig`（GRUB 收尾）、`checkupdates`（更新检查）
- `pkill`、`flock`（Waybar/并发收尾）

运行时按后端解析以下可执行文件：Pacman 为 `pacman`；APT 为 `apt-get`、`apt-cache`、
`dpkg-query`；DNF 为 `dnf5` 或 `dnf`（并使用 `rpm`）；Zypper 为 `zypper`；APK 为
`apk`；XBPS 为 `xbps-query`、`xbps-install`、`xbps-remove`；AUR 为 `paru` 或 `yay`；
Flatpak、Snap、Brew、Nix 分别为同名命令。只有实际使用的路径需要安装；例如 Debian
不需要安装 `pacman`，而 Arch 的 `mirror-update` 仍需要 `reflector`。

常用命令：

```bash
packtide                 # 原 pac 默认安装 TUI
packtide QUERY           # 原 pac QUERY
packtide remove          # 原 pacr 主流程
packtide check-updates
packtide mirror-update
packtide downgrade
packtide sysup          # legacy compatibility bridge to systide
systide
```

## 包管理器矩阵

矩阵工具只操作锁定 digest 的 disposable container；容器命令使用独立的 Podman
network namespace，测试不会调用宿主 `sudo`、写宿主 `/` 或复用宿主包数据库。运行
前先检查宿主工具：

```bash
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- doctor
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- list-images
```

执行完整容器、可选后端和 Snap VM lane，并把每行 JSON 保存在证据文件：

```bash
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- all \
  --evidence .omo/evidence/package-manager-matrix.jsonl
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- audit-cleanup
```

需要缩小范围时可运行 `single --backend alpine|debian|ubuntu|fedora|rocky|alma|opensuse|void`
或 `probe-nix`；`vm-run` 单独运行 Snap lane，`vm-interrupt-test` 验证中断清理。矩阵
依赖 `podman`；Snap VM 还需要 `qemu-system-x86_64`、`qemu-img`、`/dev/kvm` 以及
`cloud-localds` 或 `xorriso`。VM 使用锁定的 Ubuntu cloud image（见
`tests/package-managers/ubuntu-cloud-image.lock`），并在 guest 内访问 Snap Store；
宿主防火墙、无网络的 CI 或 QEMU 未提供 guest egress 时，Snap lane 会报告
`unavailable`/超时，不能据此推断 Snap 后端在宿主不可用。容器 lane 的网络仅属于
disposable namespace；若环境禁止容器联网，APT、DNF、Zypper、XBPS 等需要仓库的
probe 会失败，这是环境限制而不是静默跳过。

验证命令：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

Release 构建固定使用 workspace `profile.release`（thin LTO、8 个 codegen unit、符号剥离和 abort panic）。发布记录应同时保存版本、目标架构、Rust toolchain 与两个 binary 的 SHA-256；当前记录见 `commands/release-sha256.txt`。

参考代码快照位于仓库外的 `../independent-linux-tool-reference-20260924/`，不参与构建。
