[English](package-manager-support.md) | 简体中文

# 包管理器支持

本页是包管理器扩展的运行时契约，说明解析了哪个二进制文件、触及哪个作用域以及由哪个面向用户的命令负责操作。矩阵运行器是一次性集成检查，不是后端的第二套实现。

## 后端家族

| 家族 | `BackendId` | 作用域 | 运行时命令 | 更新 argv | 状态 |
| --- | --- | --- | --- | --- | --- |
| Arch 原生 | `Pacman` | system | `pacman`（写入时使用 `sudo`） | `pacman -Su` | `packtide` 和 `systide` |
| Debian 原生 | `Apt` | system | `apt-get`、`apt-cache`、`dpkg-query` | `apt-get upgrade -y` | 原生选择器和 `systide` 更新 |
| Fedora/RHEL 原生 | `Dnf5` | system | `dnf5`（以及 `rpm`） | `dnf5 upgrade -y` | 原生选择器和 `systide` 更新；Fedora 通道 |
| Fedora/RHEL 旧版 | `Dnf4` | system | `dnf`（以及 `rpm`） | `dnf upgrade -y` | 原生选择器和 `systide` 更新；Rocky/Alma 通道 |
| SUSE 原生 | `Zypper` | system | `zypper` | `zypper update -y` | 原生选择器和 `systide` 更新 |
| Alpine 原生 | `Apk` | system | `apk` | `apk upgrade` | 原生选择器和 `systide` 更新 |
| Void 原生 | `Xbps` | system | `xbps-query`、`xbps-install`、`xbps-remove` | `xbps-install -Su` | 原生选择器和 `systide` 更新 |
| Arch 用户 | `Paru`、`Yay` | user/AUR | `paru` 或 `yay` | 由 helper 决定 | `packtide` Arch 路径 |
| Flatpak | `Flatpak` | user/system | `flatpak` | `flatpak update -y` | 可选的 `systide` 步骤 |
| Snap | `Snap` | system | `snap` | `snap refresh` | 可选的 `systide` 步骤；VM 通道 |
| Linuxbrew | `Brew` | profile | `brew` | `brew upgrade` | 可选的 `systide` 步骤；formula |
| Nix | `Nix` | profile | `nix` | `nix profile upgrade .*` | 可选的 `systide` 步骤 |

运行时解析器检查 PATH 和原生发行版身份。缺少可选命令会报告为跳过；缺少原生命令则是可操作错误。提升权限的系统操作使用命令计划权限边界，并在调用 `sudo` 前清理加载器相关环境变量。

## 面向用户的行为

`packtide` 保留与 Arch 兼容的交互界面，同时通过类型化后端注册表将检测到的非 Arch 原生安装、卸载和更新读取路由到对应后端。`mirror-update` 和 `downgrade` 保持 Arch 专属，不会变成通用的 APT/DNF/Zypper/APK/XBPS 命令。`packtide sysup` 兼容桥在所有受支持发行版上转发到 `systide`。当提供方命令存在时，非 Arch 原生选择器中会显示可选的 Snap/Brew/Nix 行，但不会加入 Arch 选择器。`packtide sysup` 只转发到 `systide`，不维护另一套更新实现。

`systide` 检测一个原生后端并运行其升级，然后按 Flatpak、Snap、Brew、Nix 的顺序尝试存在的可选后端。它记录每一步，在可选失败后继续，并在原生步骤失败时返回非零。可选失败仅为警告：原生更新成功后不会改变退出状态 0。原生失败会在尝试可选提供方前停止序列。AUR helper 有意不在此列表中。Arch 专属的 keyring 和 GRUB/Waybar 收尾钩子仅在相应命令/文件存在时启用。

类型化后端契约区分 catalog、search、installed、details、updates、install、remove、upgrade、downgrade 和 system-upgrade 能力。调用方必须处理 `UnsupportedCapability`，不能从后端名称推断支持情况。Snap 身份限定为 system 作用域，并拒绝本地 `.snap`/`--dangerous` 源。Brew cask 不是 Linux 运行时目标；Brew 通道只验证 formula 操作。Nix 事务限定在 profile 作用域，不会修改系统 store。Arch 选择器仍仅限 Pacman/AUR/Flatpak；检测到其他后端或存在矩阵覆盖，不代表支持无关的 Arch 专属命令。

## 选择器 UI 契约

选择器在一个元数据层中维护来源标签和颜色。行使用稳定的制表符分隔列表示来源、包名、版本/详情和可选的已安装标记。隐藏的行令牌携带 `BackendId`、`PackageScope`、包类型、原生 key 和提供方元数据；选择与预览必须恢复这个类型化身份，不能从可见标签推断。事务摘要包含选定的作用域和权限边界。预览显示提供方诊断，不隐藏失败的 details 命令。

列填充使用终端显示宽度，包括 CJK 名称。预览标题在窄窗格中使用有界省略号，并将包身份与显示文本分离。这些是由行/预览测试覆盖的代码级契约；完整交互式更新流程的验收另列如下。

## 类型化事务与权限契约

原生发行版身份决定命令生成：带有 `dnf5` 的 Fedora/RHEL 解析为 `BackendId::Dnf5` 和 `dnf5`；否则解析为旧版 `BackendId::Dnf4` 和 `dnf`。解析器不会因为 PATH 中存在诱饵可执行文件而选择其他发行版家族。

读取计划以调用用户身份、locale `C` 运行。系统作用域的 install、remove、upgrade 和 refresh 计划为 `Elevated`，在移除 `LD_PRELOAD` 和 `LD_LIBRARY_PATH` 后通过可信权限运行器执行；用户 AUR、Brew 和 Nix profile 计划保持 `User`。Snap 为 system 作用域，其 install/remove/refresh 计划也为 `Elevated`。因此 `systide` 的升级命令分别是 DNF5 的 `dnf5 upgrade -y`、DNF4 的 `dnf upgrade -y` 和 Snap 的 `snap refresh`。不支持的能力会在执行命令前失败。

## 验证边界

单元测试和伪造命令测试覆盖类型化身份、精确 argv、提供方诊断、locale、权限和选择器渲染。它们不证明每个发行版上的每个真实后端操作都已通过。历史本地开发日志不会作为公共发布证据，因此本页不附加 PASS 声明。

声称矩阵覆盖的发布必须为同一次运行保留源代码提交、锁定的镜像元数据、JSON 结果和清理输出。网络失败、虚拟机工具不可用和不完整的客户机探测仍是失败或环境限制，绝不能视为通过。真实 TUI 验收和特权事务需要单独的一次性环境检查。

## 一次性矩阵

锁定的容器通道定义于 `tests/package-managers/images.lock`：

| 通道 | 镜像家族 | 后端 | 执行的操作 |
| --- | --- | --- | --- |
| `alpine` | Alpine 3.20 | APK 2.14.4 | version、search、details、install、remove |
| `debian` | Debian 12-slim | APT 2.6.1 | version、update、search、details、install、remove |
| `ubuntu` | Ubuntu 24.04 | APT 2.8.3 | version、update、search、details、install、remove |
| `fedora` | Fedora 41 | DNF5 5.2.17.0 | list、search、details、install、remove |
| `rocky` / `alma` | EL9 | DNF4 4.14.0 | list、details、install、remove |
| `opensuse` | Leap 15.6 | Zypper 1.14.94 | search、details、install、remove |
| `void` | Void musl | XBPS 0.59.1 | sync、list、search、details、install、remove |

`all` 还会加入固定版本的 Homebrew 和 Nix 镜像以及 Snap VM。容器使用 Podman `--rm`、生成的 `pm-matrix-*` 名称和桥接网络运行，具有一次性特征，不会触及主机包数据库。中断运行后必须执行 `audit-cleanup`；它检查容器、QEMU 进程、临时 overlay 和生成的 Podman 网络。

Snap VM 独立运行，因为 Snap 需要运行中的 `snapd` 和 Snap Store 访问。它使用 `tests/package-managers/ubuntu-cloud-image.lock` 中的不可变溯源信息、QEMU/KVM 和 cloud-init seed。VM 通道需要客户机网络出口；主机防火墙、离线 CI、缺少 `/dev/kvm` 或缺少 `cloud-localds`/`xorriso` 都会使该通道不可用。该结果记录为环境限制，不会改变主机后端支持契约。

## 命令

在仓库根目录执行：

```bash
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- doctor
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- list-images
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- all --evidence evidence/package-manager-matrix.jsonl
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- single --backend fedora --evidence evidence/fedora.json
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- probe-nix --evidence evidence/nix.json
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- verify-cloud-image
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- vm-run
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- vm-interrupt-test
cargo run --manifest-path tools/package-manager-matrix/Cargo.toml -- audit-cleanup
```

使用 `cargo test -p package-manager-matrix` 测试解析器、锁定信息、超时和清理逻辑。矩阵命令可能需要网络访问以拉取固定镜像和包仓库；不要用浮动镜像标签替代锁定摘要，除非同时更新锁定文件和捕获的证据。
