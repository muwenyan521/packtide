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

## 当前状态

当前是 Rust workspace，包含两个 release binary：`packtide` 和 `systide`。

共享边界位于 `crates/system-tools-core`，提供结构化命令执行、特权环境清理、可执行文件解析和原子 TTL 缓存。`packtide` 的 CLI、包源、UI/preview、事务和系统更新入口已按职责拆到 `cli.rs`、`sources.rs`、`ui.rs`、`transaction.rs`、`commands.rs`；`systide` 的 CLI、消息、新闻、UI 和更新操作分别位于对应模块。

基础运行依赖：

- `fzf`
- `pacman`
- `paru` 或 `yay`
- `sudo`（执行系统事务时）

按功能可选：

- `flatpak`
- `reflector`
- `quicksave`、`findmnt`
- `grub-mkconfig`
- `checkupdates`
- `pkill`、`flock`

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

验证命令：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

Release 构建固定使用 workspace `profile.release`（thin LTO、8 个 codegen unit、符号剥离和 abort panic）。发布记录应同时保存版本、目标架构、Rust toolchain 与两个 binary 的 SHA-256；当前记录见 `commands/release-sha256.txt`。

参考代码快照位于仓库外的 `../independent-linux-tool-reference-20260924/`，不参与构建。
