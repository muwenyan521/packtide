<div align="right">
  <a href="README.md">English</a> | 简体中文
</div>

<div align="center">

# packtide · systide

**实用的 Linux 软件包与系统维护工具**

一套类型安全的后端、两个职责清晰的命令行工具，以及基于 `fzf` 的快速交互流程，
用于在多种 Linux 发行版上完成日常软件包维护。

[![许可证：MPL-2.0](https://img.shields.io/badge/license-MPL--2.0-blue.svg)](LICENSE)
[![Rust 1.88+](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org/)

</div>

> 交互模型参考了原项目的行为。本仓库是独立的 Rust 实现。

## 目录

- [功能](#功能)
- [安装](#安装)
- [支持的后端](#支持的后端)
- [快速开始](#快速开始)
- [运行依赖](#运行依赖)
- [开发](#开发)
- [验证边界](#验证边界)
- [许可证](#许可证)

## 功能

### `packtide`

交互式软件包浏览和事务前端。它打开可搜索的选择器，保留不受显示格式影响的内部
软件包身份，展示预览，并且只在确认后执行对应后端的事务。

- 通过检测到的原生后端安装、删除、更新和查看软件包。
- 优先使用缓存结果；查询结果缺失或过期时异步刷新，输入过程保持流畅。
- 在工具存在时显示 Flatpak、Snap、Homebrew 和 Nix 的可选结果。
- 只在后端具备对应能力时启用 Arch 专属的镜像和降级流程。
- 使用 `packtide sysup` 作为可移植的 `systide` 转发命令。

### `systide`

系统更新编排器：先执行一次明确的原生更新，再执行可用的可选软件源更新。它会
本地化整个流程、报告每个事务并保留失败诊断。

- 根据发行版和可用工具检测原生包管理器。
- 原生更新优先执行；原生步骤失败时停止整个序列。
- 按固定顺序尝试可用的 Flatpak、Snap、Brew、Nix。
- 原生更新成功后，可选失败只产生警告，并继续执行后续可选后端。
- Arch 专属的新闻、镜像、快照、引导加载器和状态钩子保持条件执行。

## 安装

### 一键安装（AUR 注册暂停期间适用于所有发行版）

由于 AUR 暂停新用户注册，Arch 用户暂时也使用此脚本。安装脚本会自动检测发行版、glibc/musl 运行时、架构、权限工具和安装路径，选择对应的
GNU 或静态 musl 发布文件，补全 `fzf` 等依赖并把命令目录加入 `PATH`。如果发现旧的
`pac`、`pacr`、`pacrrr` 或 `sysup`，会先询问是否备份并替换。

```sh
curl -fsSL https://raw.githubusercontent.com/muwenyan521/packtide/master/install.sh | sh
```

正式缩写是：`ptd` 对应 `packtide`，`suu` 对应 `systide`。它们在 2026-10-07 通过本机
命令空间和 crates.io 名称检查，项目自身没有重复注册；完整命令仍然保留。

设置 `PACKTIDE_UI_LANG=zh` 使用中文安装提示，设置 `PACKTIDE_UI_LANG=en` 使用英文提示。

### 发布归档

下载 [`x86_64-unknown-linux-gnu` 发布归档](https://github.com/muwenyan521/packtide/releases)并验证对应的 SHA-256 文件，然后将两个二进制文件放入 `PATH`。归档还包含 man 页面、Shell 补全、`LICENSE`、`NOTICE.md` 和发布溯源信息。

### 从 checkout 安装

```bash
cargo install --path crates/packtide --locked
cargo install --path crates/systide --locked
```

要同时安装 man 页面和补全文件，可以使用 staging 目录：

```bash
DESTDIR="$PWD/stage" PREFIX=/usr packaging/install.sh
```

### Arch 打包

`packaging/packtide/PKGBUILD` 可以直接从 checkout 构建。提交到公开包仓库前，应切换到该
仓库要求的带标签发布源。

## 支持的后端

| 后端 | 原生软件包操作 | 系统更新 | 额外作用域 |
| --- | --- | --- | --- |
| Pacman | 安装、删除、搜索、更新、降级 | `pacman -Su` | AUR helper、镜像流程 |
| APT | 安装、删除、搜索、更新 | `apt-get upgrade -y` | Debian 和 Ubuntu |
| DNF 5 | 安装、删除、搜索、更新 | `dnf5 upgrade -y` | Fedora 和 RHEL 系列 |
| DNF 4 | 安装、删除、搜索、更新 | `dnf upgrade -y` | Rocky 和 Alma Linux |
| Zypper | 安装、删除、搜索、更新 | `zypper update -y` | openSUSE |
| APK | 安装、删除、搜索、更新 | `apk upgrade` | Alpine Linux |
| XBPS | 安装、删除、搜索、更新 | `xbps-install -Su` | Void Linux |
| Flatpak | 可选软件包行 | 可选更新 | 用户或系统作用域 |
| Snap | 可选软件包行 | 可选更新 | 系统作用域 |
| Brew | 可选 formula 行 | 可选更新 | Linuxbrew 用户作用域 |
| Nix | 可选 profile 行 | 可选更新 | profile 作用域 |

每项能力都会独立检查。检测到后端并不代表它支持所有操作。精确命令、作用域和已知
边界见[完整支持契约](docs/package-manager-support.zh-CN.md)。

## 快速开始

```bash
packtide install ripgrep
packtide check-updates
packtide remove
systide
```

常用参数：

```text
packtide install [QUERY...] [--refresh] [-y] [--no-ai]
packtide check-updates [--refresh]
packtide remove [QUERY...]
systide [--list] [--ui-lang auto|zh|en] [--news-source SOURCE] [--count N]
```

## 运行依赖

- `fzf >= 0.74.0`，用于交互式选择器。
- 当前发行版的原生软件包管理器命令。
- 系统作用域事务需要 `sudo`。
- 只有需要对应结果或更新时才需要安装可选后端命令。

语言会根据 locale 环境自动选择。`packtide` 可通过 `PACKTIDE_UI_LANG=auto|zh|en` 覆盖，
`systide` 可通过 `--ui-lang auto|zh|en` 覆盖。

## 开发

修改子系统前先阅读对应的 [AGENTS.md](AGENTS.md)。重要边界包括类型化后端身份、明确的
命令计划、权限路由，以及不会触碰主机软件包数据库的 fake-command 测试。

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release --locked
```

测试选择见 [CONTRIBUTING.zh-CN.md](CONTRIBUTING.zh-CN.md)，发布门禁见 [docs/releasing.zh-CN.md](docs/releasing.zh-CN.md)。
软件包矩阵运行器使用锁定的一次性镜像；网络或 VM 通道不可用时会报告未完成，不会静默标记为通过。

## 验证边界

自动化测试覆盖解析器契约、精确 argv、作用域和权限、本地化行、查询取消、可选后端
诊断以及类型化身份往返。fake-command 测试通过并不代表真实发行版已经接受了特权事务。
在声明发行版支持前，仍需在一次性环境中运行真实软件包管理器和 VM 通道。

## 许可证

本 workspace 使用 [Mozilla Public License 2.0](LICENSE)。源码来源和第三方关系说明见
[NOTICE.md](NOTICE.md)。
