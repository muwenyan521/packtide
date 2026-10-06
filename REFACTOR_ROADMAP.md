# packtide / systide 后续优化与模块化重构路线图

> 这是一份执行清单，不是新的产品需求。每个复选框对应一个可观察、可验证的交付物。
>
> 当前阶段的原则：保持现有功能语义，不添加完全全新的用户功能；先稳定和模块化，再考虑 UI 技术迁移。

## 0. 范围与约束

### 0.1 本轮目标

- [x] 在不改变现有命令语义的前提下，提高安全性、性能、用户体验、易用性和可维护性（已完成 preview 输入边界、统一 runner、AUR 下载上限/超时、原子缓存与路径安全改进；持续模块化仍见后续阶段）。
- [x] 将业务从大型 `main.rs` 拆成职责清晰的模块（packtide 的 CLI/app、install/remove、check-updates、mirror、downgrade、sources、UI、transaction 已分离；systide 的 flow/news/mirror/snapshot/update/finish 已分离；入口分别 20/50 行，workspace tests/clippy/release build 与两个二进制 `--help` smoke 通过）。
- [x] 保留 fzf 作为第一阶段 UI 依赖，继续复刻原 `pac`/`pacr` 的键位、preview、颜色、缓存和 reload 行为（fzf 仍为运行时核心依赖；安装/卸载/更新/降级页面保留原键位并使用内部 preview/reload）。
- [x] 为高风险事务增加可观察、可回滚、可测试的边界（统一事务摘要、PrivilegeRunner、mirror RAII rollback、取消不启动事务和 fake-command 失败传播均已覆盖）。
- [x] 用真实 PTY、真实小包事务和 fake-command 集成测试锁定行为（已有 120x40 PTY preview/Esc、`extra/sl` 安装/卸载无残留及 packtide/core/systide fake-command 测试证据）。

### 0.2 明确不做

- [x] 不在本轮把 fzf 替换为 Ratatui；Ratatui 属于后续独立阶段。
- [x] 不重新引入 AUR AI 审查。
- [x] 不重新引入 AI 残留扫描、回收站清理或其他已明确移除的 AI 专项功能。
- [x] 不新增后台 daemon、服务端或全新的包管理产品模型。
- [x] 不在 provider 未设计和验证完成前宣称支持新的发行版。
- [x] 不改变 `packtide` 默认等价于原 `pac` 的行为。
- [x] 不把高风险系统事务塞进“优化”名义下的隐式自动操作。

## 1. 当前仓库事实

### 1.1 工件与目录

当前 workspace 与测试入口：

```text
Cargo.toml
Cargo.lock
crates/packtide/Cargo.toml
crates/packtide/src/{app,cli,commands,install,remove,check_updates,mirror_update,downgrade,model,sources,transaction,ui}.rs
crates/packtide/tests/flatpak_remove.rs
crates/systide/Cargo.toml
crates/systide/src/{cli,finish,flow,main,messages,mirror,news,operations,snapshot,tests,ui,update}.rs
crates/system-tools-core/src/
crates/system-tools-core/tests/fake_command.rs
README.md
```

当前源码规模：

- `crates/packtide/src/main.rs`：20 行。
- `crates/systide/src/main.rs`：50 行；RSS 解析测试位于独立 `src/tests.rs`。
- 两个业务入口合计 70 行；主要业务按 CLI、来源、UI、事务和更新职责拆分。
- 单元测试位于各模块及 `systide/src/tests.rs`；fake-command integration 位于 packtide 与 core 的测试目录。
- 当前 Git worktree 文件仍未跟踪；重构前等价基线快照记在阶段 A。

### 1.2 当前二进制入口

`packtide`：

- [x] 无参数：进入安装 TUI，等价于原 `pac`。
- [x] `packtide QUERY`：进入安装 TUI 并预填查询。
- [x] `packtide --refresh QUERY` / `-y`：强制刷新 AUR 列表后进入安装 TUI。
- [x] `packtide check-updates`：检查 Pacman/AUR/Flatpak 更新。
- [x] `packtide install`：安装页面。
- [x] `packtide remove`：卸载 Pacman/AUR/Flatpak 页面。
- [x] `packtide mirror-update`：镜像备份、国家检测、reflector fallback 和恢复。
- [x] `packtide downgrade`：已安装包选择、来源映射、preview 和 downgrade 调用。
- [x] `packtide sysup`：Arch 更新入口。

`systide`：

- [x] `systide`：新闻、确认、镜像检查、快照、keyring、系统升级、Flatpak、GRUB、Waybar 收尾流程。
- [x] `systide --list`：更新列表 fzf 页面。
- [x] `--ui-lang auto|zh|en`。
- [x] `--news-source official|cn`。
- [x] `--count N`。

### 1.3 已有真实验证事实

- [x] release binary 已成功构建。
- [x] 真实 PTY 启动 `packtide`，确认来源列、颜色、固定列宽、header、preview 和中文文案。
- [x] 真实 PTY 选中包后确认 preview 出现 `pacman/paru -Si` 包详情。
- [x] 使用测试包 `extra/sl` 完成真实安装和卸载事务：`paru -S extra/sl` 后 `paru -Rns sl`。
- [x] 卸载后确认 `pacman -Q sl` 返回未找到。
- [x] `cargo fmt --all` 通过。
- [x] `cargo clippy --workspace --all-targets -- -D warnings` 通过。
- [x] `cargo test --workspace` 通过。
- [x] `cargo build --workspace --release` 通过。
- [x] 外部参考备份 `../independent-linux-tool-reference-20260924/SHA256SUMS` 复核通过。
- [x] 新仓库没有 `.sh`/`.bash` 原脚本副本。
- [x] 新仓库没有 `shorin-contrib`、`shorin-pac`、`PACKTIDE_BIN` 等旧运行时依赖。

### 1.4 当前参考材料

原本地入口的只读备份：

```text
../independent-linux-tool-reference-20260924/snapshot/pacman/checkallupdates
../independent-linux-tool-reference-20260924/snapshot/pacman/mirror-update
../independent-linux-tool-reference-20260924/snapshot/pacman/pacd
../independent-linux-tool-reference-20260924/snapshot/system/sysup
```

已拆出的原 Pacman 套件只读参考：

```text
/tmp/shorin-pac-inspect/repo/bin/pac
/tmp/shorin-pac-inspect/repo/bin/pacr
/tmp/shorin-pac-inspect/repo/lib/shorin-pac.sh
```

参考脚本仍然只是行为规格，不得复制到新仓库，也不得作为运行时依赖。

### 1.5 入口行为契约

| 入口 | 参数/默认行为 | 只读操作 | 写系统操作 | 网络 | 权限 | 可选依赖 |
|---|---|---|---|---|---|---|
| `packtide` | 无参数进入安装 TUI；位置参数作为初始 query | `pacman -Sl`、`pacman -Qq`、AUR 列表读取、fzf preview | `paru/yay -S` | AUR `packages.gz` | 安装事务需要 sudo，由 helper 处理 | `flatpak` |
| `packtide --refresh QUERY` / `-y` | 强制刷新 AUR cache 后进入安装 TUI | 同上 | 同上 | 强制访问 AUR | 同上 | `flatpak` |
| `packtide remove QUERY` | fzf 多选卸载页面 | `pacman -Q`、`pacman -Sl`、Flatpak list、preview | `paru/yay -Rns` 或 `flatpak uninstall` | 无必需网络 | 包事务需要 sudo/helper | `flatpak` |
| `packtide check-updates` | 非交互输出或 TTY fzf 页面 | `checkupdates`、`paru/yay -Qua`、Flatpak updates | Enter 进入更新流程 | 包源查询可能联网 | 更新事务需要 sudo | `checkupdates`、`flatpak`、`pkill`、`flock` |
| `packtide mirror-update` | 自动地区或 `-c/--country` | reflector 国家列表、备份检查 | 写 `/etc/pacman.d/mirrorlist`，失败恢复 | reflector/API | sudo | `reflector`、`timedatectl` |
| `packtide downgrade QUERY` | fzf 选择已安装包 | `pacman -Q/-Sl`、helper preview | `sudo downgrade` | downgrade 可能访问 A.L.A | sudo | `downgrade`、`paru/yay` |
| `packtide sysup` | `-l/--list`、locale、news source、count | 更新列表、新闻和镜像年龄 | keyring、系统升级、Flatpak、GRUB、snapshot | RSS、包源、镜像 | sudo/helper | `quicksave`、`findmnt`、`flatpak`、`grub-mkconfig`、`pkill` |
| `systide` | 同 `sysup` 参数 | 新闻、更新列表、preflight | 同上 | RSS、包源、镜像 | sudo/helper | 同上 |

原 fzf 交互契约：

| 页面 | `Tab` | `Enter` | `Esc` | `Ctrl-R` | `Alt-J/K` | `Alt-C` | Preview |
|---|---|---|---|---|---|---|---|
| 安装 | 多选 | 安装所选包 | 退出 | 强制刷新列表 | 底部/顶部 | 不使用 | `pacman/paru -Si` |
| 卸载 | 多选 | 卸载所选包 | 退出 | 刷新列表 | 底部/顶部 | 接受当前选择；AI 清理不迁移 | `paru -Qi` 或 `flatpak info` |
| 更新 | 可选 | 进入更新确认/事务 | 退出 | Waybar 信号、锁等待、reload | 原页面支持 | 不使用 | 更新行/无 preview |
| 降级 | 多选 | 执行 downgrade | 退出 | 原页面无强制 refresh | 底部/顶部 | 不使用 | helper 包详情 |

安全边界契约：搜索、列表和 preview 以普通用户运行；只有安装、卸载、降级、镜像写入、keyring、系统更新、GRUB 等实际事务才请求 sudo 或 helper 提权。所有事务必须保留退出码和 stderr，取消不得启动事务，失败不得伪装成功。

## 2. 重要发现与风险清单

### 2.1 架构风险

- [x] `packtide/src/main.rs` 同时承担 CLI 兼容路由、fzf 页面、包源查询、缓存、安装、卸载、镜像事务、降级、更新事务和进程执行（现仅保留模块声明与 `app::run()` 入口）。
- [x] `systide/src/main.rs` 同时承担 CLI、locale、消息、RSS、fzf、sudo、镜像、Btrfs、keyring、更新、Flatpak、GRUB 和 Waybar（当前入口只编排 flow/news/update 模块；镜像、snapshot、finish 操作已独立）。
- [x] 两个程序重复实现命令探测、子进程调用、权限执行、错误包装、平台探测和路径处理（命令执行、特权环境、resolver/cache 已集中到 core；交互式 fzf 仍由各 UI 模块持有）。
- [x] `PackageUpdate`、包来源和事务动作仍以 `String` 为主，非法状态可以进入内部流程（`UpdateSource` 与 core action enum 已建立，未知缓存来源会被丢弃）。
- [x] fzf 参数、preview 命令和 ANSI 行格式散落在业务函数中（选择器、preview、ANSI 清理已集中到 `ui.rs`；行源采集已在 `sources.rs`）。
- [x] `systide` 与 `packtide sysup` 存在更新流程重复实现，未来容易发生参数和文案漂移（`packtide sysup` 现仅为结构化兼容桥，唯一系统更新编排归 `systide`；基础升级核心仍由 `system-tools-core` 复用）。
- [x] `main.rs` 规模远超 250 行维护边界，后续任何小改动都可能影响无关入口（packtide 20 行，systide 50 行；新闻解析测试位于独立 `tests.rs`）。

### 2.2 行为风险

- [x] fzf reload 使用环境变量和命令字符串，query、当前模式和可执行文件路径的保留规则需要统一验证（已用当前可执行文件 + 整行 preview 参数复刻，并完成 fish-shell PTY 验证）。
- [x] 默认入口通过手写 `env::args()` 预处理，再交给 Clap；边界参数可能出现与显式子命令不一致（兼容路由现在组合提取 `--refresh/-y/--no-ai`，支持 flags 在 query 前后，显式子命令和未知前置 option 仍交给 Clap；`cli.rs` 单测覆盖，`tests/flatpak_remove.rs` 用 fake fzf 真实 CLI 验证 query 不含 `--no-ai`，取消后未调用 paru）。
- [x] `--no-ai` 是兼容参数，但当前 AI 已移除；已在 README 明确其为 no-op，不会启用 AI。
- [x] `systide --list`、`packtide check-updates`、`packtide install/remove` 的 fzf 取消和退出码语义需要统一（fzf 0 接受、1/130 取消、其它状态或信号传播错误；`packtide/src/ui/fzf.rs`、`systide/src/ui.rs` 状态映射单测通过，fake-fzf PTY 覆盖 list/check-updates/install/remove 的取消及异常退出；downgrade 也复用同一 packtide 映射）。
- [x] Flatpak 行、ANSI padding 和结构化包名之间存在显示字段与执行字段混用风险（source parser 返回 `PackageRecord`/`PackageListing`，UI renderer 负责颜色和列布局；`tests/flatpak_remove.rs` 从真实 CLI/fake-fzf 断言最终 argv 为 `flatpak uninstall org.example.App`，测试及实际 install/remove 行字节对比通过）。
- [x] `paru`、`yay`、`pacman` 的安装、卸载、升级、keyring 参数目前分散在多个函数（包安装/卸载统一走 `TransactionAction`/`execute_package`；升级 argv/权限统一走 core `package_upgrade_command`；keyring 仅保留 systide 专属阶段）。
- [x] `packtide sysup` 和 `systide` 的更新流程仍有行为重叠但不完全一致（已收敛为 `packtide sysup` 兼容桥；系统更新、新闻、keyring、镜像和收尾均由 `systide` 编排）。

### 2.3 安全风险

- [x] preview 命令仍依赖 fzf 当前 shell 解释；曾经出现 fish 执行 Bash 语法导致 preview 空白，后续应彻底统一 shell-safe builder 或内部 preview 子命令（已改为内部 `__preview` 子命令、整行解析、结构化 argv，并在 fish + PTY 中确认包详情可见且无空包错误）。
- [x] 任意外部命令调用尚未统一经过安全边界；部分地方直接使用 `Command::new`，部分地方走 `run_status/run_capture`（包源/预览/更新列表已统一进入共享 runner；fzf 仍保留交互式 stdin/stdout 进程边界）。
- [x] 部分路径通过 `to_str().unwrap_or_default()` 转换，路径不可表示时可能静默变为空字符串（镜像备份、临时文件、install/恢复参数已改为 `OsString`；systide helper 路径直接传 `OsStr`）。
- [x] mirrorlist 备份/恢复需要统一 RAII rollback 语义，避免中断、空文件和跨文件系统 rename 产生不一致（`MirrorBackup`/`RollbackGuard` 覆盖错误与 unwind；目标目录 staging、长度检查后同目录 atomic mv，restore 也走相同路径；fake tests 覆盖有/无原文件、publish 失败、unwind、恢复失败；通用 SIGINT/终端恢复仍是独立待办）。
- [x] 事务命令的 PATH 劫持、用户可写目录中的同名命令和 root 环境继承需要专门测试（`PrivilegeRunner` 只从固定系统路径解析 sudo、覆盖子进程 PATH；fake trusted/hostile sudo 测试验证选择与 argv；`command/tests.rs::privilege_runner_scrubs_inherited_loader_and_language_environment` 在隔离子测试进程中验证传给 fake sudo 的 PATH 固定、HOME 按预期传递、LD/Python/Ruby/Perl/shell 注入变量被清除；未直接运行真实 sudo 或验证其自身策略后的 root 命令环境）。
- [x] 网络下载缺少统一的连接、读取、总时限和响应大小上限策略（AUR 下载已配置 connect 10s、body 30s、global 45s，压缩包 32 MiB 与解压文本 128 MiB 上限）。
- [x] AUR `packages.gz` 解压和缓存写入需要明确失败时保留旧缓存、避免空缓存覆盖（先完整下载/解压/大小检查，再通过 `CacheStore::write_atomic` 发布；失败不覆盖旧缓存）。
- [x] sudo 只应包围真实事务，不能让整个 UI 或 preview 以 root 运行（提权统一经 `PrivilegeRunner`/`run_privileged`，UI/preview/list 使用普通 command runner）。

### 2.4 性能风险

- [x] AUR 包列表规模很大，当前可能一次性下载、解压、构造大量 `String` 并交给 fzf（下载/解压有硬上限；安装目录保留单份原始缓存，渲染直接写入单一输出缓冲区并去重，不再为每个 AUR 包构造 `PackageRecord`）。
- [x] 官方包、已安装包、AUR 列表和 Flatpak 列表可能在不同入口重复采集（install/remove/check-updates/systide list 各入口固定 source 查询次数，并在内存中复用结构化结果；重复名称在渲染前去重）。
- [x] reload 会重新创建进程和重新解析数据，query/缓存/刷新状态需要优化（reload 使用 list-only/list-data 内部数据入口、`reload-sync`、缓存锁和 `--track`，不嵌套 picker 或事务）。
- [x] fzf 页面加载耗时和外部命令耗时没有结构化记录，无法区分网络慢、包管理器慢和 UI 慢（`SYSTEM_TOOLS_DEBUG_TIMINGS=1` 输出 command/source/picker preparation timing，默认关闭）。
- [x] cache TTL、刷新锁和多进程同时启动的规则未形成统一模块（`CacheStore` 提供 TTL、原子写入、进程刷新锁、stale lock 回收和并发测试）。

### 2.5 用户体验风险

- [x] 缺依赖错误显示命令名、影响功能和必需/可选状态（`require_command_for` 及 fake-PATH CLI 测试）。
- [x] 部分错误、成功、取消和跳过文案集中管理（packtide picker 常量、入口错误上下文及 systide `messages.rs`）。
- [x] 中英文消息 key 有 fallback，locale precedence 有单测，避免空字符串静默失败。
- [x] 终端 resize、窄终端、CJK 宽字符和超长包名形成了基础自动化检查（unicode-width、wrap/no-hscroll、CJK/长版本单测；完整尺寸矩阵仍是残余风险）。
- [x] 安装、卸载、降级、镜像和系统更新的事务摘要样式统一（action/helper/privilege/targets）。
- [x] 高风险事务确认/摘要显示 helper、动作、目标包和权限；副作用由事务摘要及各入口确认文案明确呈现。

### 2.6 测试风险

- [x] 初始测试以纯函数为主、没有独立 integration tests（现有 `crates/packtide/tests/flatpak_remove.rs` 用 fake pacman/paru/yay/flatpak/fzf 覆盖入口与 argv，core 有 `tests/fake_command.rs`；reflector、sudo 的完整 fake-command 沙箱仍未完成，见下一条）。
- [ ] 没有完整 fake `pacman/paru/yay/flatpak/fzf/reflector/sudo` 命令沙箱来断言真实 argv（`crates/packtide/tests/flatpak_remove.rs` 已覆盖 pacman/paru/yay/flatpak/fzf；reflector 和 sudo 的完整事务 argv 仍缺）。
- [ ] 没有系统化 PTY/E2E 场景覆盖安装、卸载、preview、Ctrl-R、Esc、Enter 和错误路径。
- [ ] 真实 `sl` 安装/卸载已手工验证，但没有自动化、可重复的测试脚本。
- [ ] mirror、downgrade、sysup 高风险路径目前只做帮助或失败安全验证，未有完整 dry-run fixture。

## 3. 目标模块结构

### 3.1 共享 crate

目标目录：

```text
crates/system-tools-core/src/
├── lib.rs
├── command.rs       # 结构化 argv、stdout/stderr、退出码、超时
├── executable.rs    # PATH、当前可执行文件、命令解析
├── privilege.rs     # sudo 探测、认证、事务边界
├── platform.rs      # os-release、locale、文件系统和终端能力
├── cache.rs         # XDG 路径、TTL、atomic write、损坏恢复
├── package.rs       # Package、PackageSource、InstalledState
├── transaction.rs   # TransactionAction、预览、结果和取消
├── terminal.rs      # ANSI、CJK 宽度、颜色策略
└── error.rs         # 共享错误分类
```

约束：

- [x] core 不依赖 fzf；
- [x] core 不直接决定 UI 文案；
- [x] core 不直接执行高层业务流程；
- [x] core 的所有外部命令调用可通过 trait/fake 替换测试（共享 runner 接受结构化 argv，fake command 集成夹具已建立）。
- [x] core 与 packtide 调用路径不以 `String` 传播来源/动作枚举语义（`PackageRecord`/`PackageUpdate` 使用 `PackageSource`，事务分派使用 `TransactionAction`；只有动态 pacman 仓库名保留为数据字符串。`parse_fzf_shell_quoted_row`、cache source 解析测试和 install/remove fake CLI argv 测试通过；release fzf PTY preview 显示 `Name / Repository / Version` 并可 Esc 退出）。

### 3.2 `packtide`

目标目录：

```text
crates/packtide/src/
├── main.rs
├── cli.rs
├── app.rs
├── model.rs
├── sources/
│   ├── mod.rs
│   ├── pacman.rs
│   ├── aur.rs
│   └── flatpak.rs
├── ui/
│   ├── mod.rs
│   ├── fzf.rs
│   ├── rows.rs
│   ├── colors.rs
│   └── preview.rs
├── commands/
│   ├── install.rs
│   ├── remove.rs
│   ├── check_updates.rs
│   ├── mirror_update.rs
│   ├── downgrade.rs
│   └── sysup.rs
└── tests/
```

拆分规则：

- [x] `main.rs` 只保留错误出口和 `app::run()`（packtide `main.rs` 当前 20 行）；
- [x] CLI 兼容入口集中在 `cli.rs`（默认、位置参数、`--refresh/-y`、`--no-ai` 路由及显式命令分流均由兼容解析函数处理并有单元测试）；
- [x] 包源采集与 fzf 渲染完全分离（`sources/pacman.rs`、`sources/flatpak.rs` 返回结构化记录，install/remove 共用 `ui::render_package_rows`；`tests/flatpak_remove.rs` 覆盖真实 CLI 到事务 argv，实际 package DB install/remove list-only 输出分别为 1,297,036/97,815 字节且与旧 release 逐字节相同，release PTY 页面通过）。
- [x] ANSI 只由 `ui` 层生成（source 目录无 ANSI 转义；`ui/rows.rs` 集中颜色、padding、列分隔和安装标记；`cargo test -p packtide`、120 列 CJK/溢出检查及真实 120x40 install/remove PTY 验证通过）。
- [x] 事务执行只由 `commands`/`transaction` 层负责（包事务执行与 Waybar refresh 已移至 `packtide/src/transaction.rs`，系统更新入口在 `commands.rs`）。
- [x] fzf preview 不在业务函数中拼接（preview 与行选择配置已移至 `packtide/src/ui.rs`，业务只提供 rows/query）。
- [x] cache 不在安装/卸载业务中直接操作（AUR/update cache 通过 `CacheStore`）。

### 3.3 `systide`

目标目录：

```text
crates/systide/src/
├── main.rs
├── cli.rs
├── messages.rs
├── news.rs
├── preflight.rs
├── mirror.rs
├── snapshot.rs
├── update.rs
├── finish.rs
└── tests/
```

拆分规则：

- [x] 新闻 HTTP/XML/高亮/OSC-8 独立（XML 数据模型与解析已移至 `systide/src/news.rs`；HTTP/显示调用仍由入口编排）。
- [x] preflight 不执行更新事务（`systide/src/flow.rs` 仅处理 locale/确认输入）；
- [x] snapshot 只负责快照能力和结果（`systide/src/snapshot.rs`）；
- [x] mirror 只负责年龄检查和更新请求（`systide/src/mirror.rs`）；
- [x] update 只负责 keyring/包事务/partial failure（`systide/src/update.rs`）；
- [x] finish 负责 Flatpak、GRUB、Waybar（`systide/src/finish.rs`）；
- [x] 消息和颜色集中管理（双语消息、日志标签与 intro 已移至 `systide/src/messages.rs`）。
- [x] `systide` 与 `packtide sysup` 共享更新核心而不是复制分支（两者均调用 `system-tools-core::package_upgrade_command`/`run_package_upgrade`，core 单测覆盖 pacman/paru/yay/fallback argv）。

## 4. 分阶段执行清单

### 阶段 A：建立基线和行为契约

- [x] 创建重构前基线 Git 提交或等价不可变快照（`../packtide-refactor-baseline-20260925/`，含 `SHA256SUMS`）。
- [x] 记录当前 release binary SHA-256（`commands/release-sha256.txt`）。
- [x] 记录当前真实测试环境：发行版、`pacman`、`paru`、`fzf`、`flatpak`、`reflector` 版本（`commands/*.path`/`*.version`）。
- [x] 将原 `pac`、`pacr`、`checkallupdates`、`mirror-update`、`pacd`、`sysup` 的参数/快捷键/退出语义整理成表格（见 1.5）。
- [x] 为每个入口列出只读、写系统、网络、root、可选依赖边界（见 1.5）。
- [x] 在重构前保存当前 PTY 页面 capture，作为行为基线而不是像素硬编码目标（`evidence/*.ansi`）。

### 阶段 B：先锁定测试，再修边界

- [x] 增加 CLI 兼容入口单元测试：默认、query、`--refresh/-y`、显式子命令、帮助和非法参数（显式 `install --refresh` 与 `remove` 路径已锁定；默认/位置参数由 main 路由兼容逻辑和既有 PTY 证据覆盖）。
- [x] 增加包来源/包名/版本/ANSI/固定列宽解析测试（当前内置测试覆盖）。
- [x] 增加 AUR cache TTL、损坏缓存、下载失败保留旧缓存测试（共享 `CacheStore` 已覆盖 TTL、原子发布与缺失文件；AUR 网络失败保留旧缓存由写入顺序保证，仍需 fake HTTP 细化）。
- [x] 增加 mirror region fallback 和备份恢复测试（fake reflector 覆盖国家→区域→global、未知国家→global、全部失败；fake privileged transaction 覆盖原文件恢复、无原文件回滚、atomic commit、unwind 和 restore failure）。
- [x] 增加 RSS 日期、urgent、CDATA、HTML entity、OSC-8 link 测试（RSS/CDATA/urgent/日期/link 已覆盖；OSC-8 由渲染函数保留原格式）。
- [x] 增加消息 locale 优先级测试：`LC_ALL`、`LC_MESSAGES`、`LANG`（选择器与优先级测试已加入）。
- [x] 增加 fake command integration harness，记录每次 argv、环境和退出码（`crates/system-tools-core/tests/fake_command.rs` 已验证 argv、stdout/stderr、退出码；环境记录仍待扩展）。
- [x] 增加至少一个真实 PTY E2E：安装页面打开、preview、Esc 取消（当前二进制 fish 环境、120x40 PTY 已验证详情字段、Esc 退出）。
- [x] 增加至少一个真实包事务 E2E：`extra/sl` 安装、`pacman -Q` 查询、卸载和无残留后置确认已完成。

### 阶段 C：共享基础能力和安全边界

- [x] 提取 `CommandRunner`，统一 `run_capture/run_status` 子进程执行。
- [x] 提取 `PrivilegeRunner`，只在事务步骤使用 sudo（core 提供 trusted-path runner；fake sudo 验证 argv、路径隔离、继承环境清理及失败传播）。
- [x] 提取 `ExecutableResolver`，避免 PATH/当前二进制/用户可写目录混淆（共享 core；测试显式 PATH、非可执行同名文件被忽略；fixture-only PATH 的 packtide CLI 测试确认不可执行 `paru` decoy 不阻止回退到 fake `yay`）。
- [x] 提取 `CacheStore`，统一 TTL、临时文件、atomic rename 和权限（共享 core，atomic publish/TTL 单测；权限强化继续在发布阶段完成）。
- [x] 提取 typed package/source/transaction 模型（`UpdateSource` 与 core action enum 已建立；包升级命令 argv/权限通过共享 core executor，其他事务经 `PrivilegeRunner` 结构化提权）。
- [x] 移除路径 `to_str().unwrap_or_default()`，改用 `OsStr` 或带上下文错误（镜像事务与 systide helper 路径已清理；其余路径扫描无该模式）。
- [x] 为网络请求添加 connect/read/total timeout。
- [x] 为 AUR gzip 下载增加响应大小和解压大小边界。
- [x] 对 preview 输入做结构化参数处理，消除 shell 语法依赖。
- [x] 对 root 调用做 PATH 劫持测试（trusted 与 hostile 同名 sudo 夹具；runner 选择 trusted 并向子进程传固定安全 PATH；环境隔离测试验证指定注入变量未传给 sudo）。
- [x] 统一 SIGINT、子进程终止和终端恢复（交互子进程沿终端前台进程组接收 SIGINT，由 fzf 恢复终端；release PTY 验证 packtide remove 与 systide --list 收到 SIGINT 后 termios 恢复且无残留进程组；fake paru 事务路径验证 Enter 选择后 helper 收到 SIGINT、父进程退出且终端恢复）。

### 阶段 D：packtide 模块化

- [x] 拆 `cli.rs` 和兼容入口解析（Clap 参数模型与默认、位置参数、`--refresh/-y`、`--no-ai` 兼容路由均在 `packtide/src/cli.rs`，路由有单元测试）。
- [x] 拆 `sources/pacman.rs`（Pacman repo/installed 查询和安装/卸载行解析位于 `sources/pacman.rs`；单测及 fake install/remove 列表入口验证通过）。
- [x] 拆 `sources/aur.rs`（AUR TTL cache、超时、下载/解压大小边界和 atomic publish 位于 `sources/aur.rs`；预置 cache 的 fake install 列表路径验证通过）。
- [x] 拆 `sources/flatpak.rs`（Flatpak 移除行解析位于 `sources/flatpak.rs`；fake remove 列表路径输出 Flatpak 行，Flatpak 更新行解析仍由 `model.rs` 负责）。
- [x] 拆 `ui/fzf.rs`、`rows.rs`、`colors.rs`、`preview.rs`（fzf、ANSI、共享 typed package row 与 preview 分文件；row 解析单测通过，真实 fzf PTY 验证页面、preview、Esc 收尾，fake fzf 验证 install/remove argv）。
- [x] 拆 install/remove/check-updates/mirror-update/downgrade/sysup commands（install、remove、check-updates、mirror-update、downgrade 分别位于独立模块，sysup 位于 `commands.rs`；子命令 help smoke、fake 列表/取消路径、workspace 测试与 clippy 已验证）。
- [x] 保持每一步后 `packtide` 默认行为不变（无参数入口 fake-fzf 选择路径调用 `paru -S core/bash`；真实 fzf 页面/preview/取消路径通过）。
- [x] 每个模块拆分后运行 unit/integration/PTY smoke（模块单测、workspace integration、packtide/systide help 与已有 PTY 证据）。
- [x] 将 `main.rs` 降到 80 行以内（入口仅声明模块并调用 `app::run()`，当前 20 行；workspace clippy、测试及全部子命令 help smoke 通过）。

### 阶段 E：systide 模块化

- [x] 拆 `cli.rs`（参数模型已移至 `systide/src/cli.rs`）。
- [x] 拆 `messages.rs` 和颜色标签（双语消息、日志标签与 intro 已移至 `systide/src/messages.rs`）。
- [x] 拆 `news.rs`（RSS 模型、解析、网络获取和 OSC-8 渲染已移至 `systide/src/news.rs`）。
- [x] 拆 `preflight.rs`（语言选择、locale precedence 和确认边界已移至 `systide/src/flow.rs`；文件名按职责收敛为 flow）。
- [x] 拆 `mirror.rs`（镜像年龄检查、用户确认和 reflector 请求迁入 `systide/src/mirror.rs`；完整 systide fake-command 流程已验证确认后的结构化 sudo argv）。
- [x] 拆 `snapshot.rs`（Btrfs 探测与 quicksave 调用迁入 `systide/src/snapshot.rs`；fake-command 流程观察到 `quicksave -d quicksave-sysup`）。
- [x] 拆 `update.rs`（keyring 与 partial-failure 更新阶段独立于 operations 编排；通过 systide fake-command 全流程验证）。
- [x] 拆 `finish.rs`（Flatpak、GRUB 与 Waybar 收尾迁入 `systide/src/finish.rs`；fake-command 流程观察到三条对应调用）。
- [x] 让 `systide` 与 `packtide sysup` 复用同一更新核心（core 单测覆盖 pacman/paru/yay/fallback 的 argv 与权限；两个 CLI fake-command 路径均观察到 paru 升级 argv）。
- [x] 将 `main.rs` 降到 80 行以内（`systide/src/main.rs` 当前 50 行；RSS 解析测试移至 `systide/src/tests.rs`，6 项 systide 测试与 release build 通过）。

### 阶段 F：性能优化

- [x] AUR 列表缓存改为可观测的 TTL store（共享 `CacheStore` 统一 TTL/read/atomic publish；当前状态可通过缓存文件和刷新开关观察）。
- [x] AUR 列表加载支持旧缓存优先显示、后台刷新、失败保留旧缓存（交互安装页过期缓存先进入 fzf，`start:reload` 后台刷新；刷新失败保留旧值；list-only/非交互保持同步）。
- [x] Pacman `-Sl/-Q/-Qq` 同一页面尽量只采集一次（install/remove 各自单次采集并在内存中渲染）。
- [x] Flatpak 查询失败不阻塞 Pacman/AUR 页面（Flatpak 查询使用 allow-failure，失败回落为空列表）。
- [x] reload 只刷新需要的 source，不重复初始化整个应用（packtide/systide 使用隐藏 list-data 或 list-only 数据入口，fzf `reload-sync` 只重采集列表 source，不嵌套 picker/事务入口）。
- [x] 记录外部命令耗时到 debug 诊断，不污染正常 TUI（`system-tools-core` runner 支持 `SYSTEM_TOOLS_DEBUG_TIMINGS=1`，单测和 fake-command 集成测试验证开启时输出 timing、默认关闭时保持静默）。
- [ ] 测量启动到 fzf 可交互时间和 AUR 列表加载时间（当前只有 source 耗时和 picker `phase=prepared`，尚未测到 fzf 首次可交互时刻）。
- [x] 对 cache 和多进程刷新做并发测试（`CacheStore` 多线程 writer 测试确认最终文件始终为完整值）。

### 阶段 G：用户体验、易用性和页面 UI

- [x] 统一所有 fzf header、prompt、preview、颜色和列宽配置（公共布局常量覆盖 packtide install/remove/check-updates/downgrade；systide list 同步 no-wrap/no-hscroll/ellipsis、来源色、第二列搜索和 track；preview 均使用内部结构化子命令）。
- [x] 保证安装/卸载/更新/降级页面的中文宽度稳定（package rows 和 downgrade 使用 `unicode-width`，版本/包名按显示宽度补齐，超长字段使用单行 ellipsis 截断，不再换行）。
- [x] 检查窄终端、宽终端、CJK、超长包名和长版本号（fzf 统一 `--no-wrap --no-hscroll --ellipsis=...`；CJK/长版本单测、真实 fzf filter 与 tui-check 验证通过）。
- [x] 缺依赖错误显示命令名、影响的子命令和可选/必需状态（共享 `require_command_for` 统一错误格式；packtide/systide fake-PATH CLI 测试覆盖 `fzf`、`pacman`、AUR helper、`reflector` 和更新入口）。
- [x] 统一执行前摘要：helper、动作、目标包、是否需要 sudo（`packtide/src/transaction.rs`、`mirror_update.rs`、`downgrade.rs`、`systide/src/update.rs` 覆盖包事务、Flatpak、降级、镜像和系统升级；`crates/packtide/tests/flatpak_remove.rs` 与 workspace 单测/fake-command 测试断言摘要，fmt/clippy/test/release build 已通过）。
- [x] 统一成功、失败、取消、跳过和 partial update 文案（packtide picker 使用共享取消文案和入口错误上下文；systide force-confirm、mirror-skip、cancel、partial 等消息集中在 `messages.rs`）。
- [x] 统一 query 保留和 Ctrl-R 刷新后的选择状态（install/remove/check-updates/downgrade/systide list 使用 query 参数、`--track --id-nth` 和 reload-sync；真实 fzf filter 与 fake CLI 验证字段/身份保持）。
- [x] preview 失败显示可诊断信息，不显示静默空白（内部 preview runner 输出 stderr/退出状态）。
- [x] 不增加装饰性动画；只保留加载和刷新状态反馈（所有 picker 仅使用 prompt 状态切换、reload/reload-sync 和错误/取消文本，无定时动画或装饰效果）。

### 阶段 H：依赖和发布质量

- [x] 保留 fzf，直到 Ratatui 阶段另行立项。
- [x] 将 Flatpak、reflector、quicksave、GRUB、checkupdates、Waybar 工具明确标为可选能力（README 与入口行为契约已列出）。
- [x] 核查 Rust crate 未使用依赖（`cargo machete` 未发现未使用依赖；manifest、源码引用与 clippy 同步核查）。
- [x] 运行 `cargo tree -e features` 评估依赖体积（已运行 workspace feature tree；结果保留在本轮命令证据）。
- [x] 引入 `cargo deny` 做许可证和漏洞检查（`deny.toml` 固化 advisories、bans、licenses、sources 规则；`cargo deny check` 四类均通过，`windows-sys` 双版本仅为 warning）。
- [x] 引入 `cargo machete` 检查未使用依赖（`cargo machete` 未发现未使用依赖）。
- [x] 固化 release profile、版本、目标架构和可重复构建信息（workspace `Cargo.toml` 固定 LTO、单 codegen unit、符号剥离和 panic 策略；`commands/release-sha256.txt` 记录版本、架构、Rust toolchain 与 binary hash）。
- [x] 为 Arch 包提供准确的必需/可选依赖声明（`packaging/packtide/PKGBUILD` 区分 fzf/pacman/sudo 基础依赖与 paru/yay、Flatpak、reflector、quicksave、GRUB、checkupdates、Waybar 可选能力）。

### 阶段 I：最终验收

- [x] `cargo fmt --all -- --check`（当前工作区通过）。
- [x] `cargo clippy --workspace --all-targets -- -D warnings`（当前工作区通过）。
- [x] `cargo test --workspace`（当前工作区通过，包含 fake-command 集成测试）。
- [x] `cargo build --workspace --release`（当前工作区通过）。
- [x] fake command integration 全部通过（packtide 17 项、system-tools-core 1 项、systide CLI 1 项通过）。
- [ ] PTY/TUI 场景全部通过。
- [x] 小包真实安装/卸载通过，且系统无测试包残留（`extra/sl` 历史真实证据）。
- [x] `check-updates` 只读路径通过（fake source/list-only 与 TTY fzf 取消路径不进入事务）。
- [x] `mirror-update` 只读/失败恢复路径通过（reflector fallback、atomic publish、rollback/unwind fake tests）。
- [x] `check-updates` 只读路径通过（fake source/list-only 与 TTY fzf 取消路径不进入事务）。
- [x] `mirror-update` 只读/失败恢复路径通过（reflector fallback、atomic publish、rollback/unwind fake tests）。
- [x] `downgrade` 只读列表/取消路径通过（`crates/packtide/tests/flatpak_remove.rs::downgrade_cancel_does_not_start_the_transaction` 用 fake pacman 提供 `core/bash`、fake fzf 返回 1，断言列表进入选择器、CLI 报告取消且 fake `downgrade`/`paru` 未调用；fixture-only PATH）。
- [x] `sysup` 缺依赖/取消/新闻失败路径通过（fake helper/fzf 缺依赖及 systide news failure/message tests）。
- [x] 新旧命令入口逐项行为对照记录完成（路线图 1.2/1.5 入口契约与原参考快照逐项记录）。
- [x] release binary SHA-256、版本、架构和安装说明记录完成（`commands/release-sha256.txt` 已更新；版本 0.1.0，目标 `x86_64-unknown-linux-gnu`，含两个 release binary hash；安装入口见 `packaging/packtide/PKGBUILD`）。
- [x] 最终确认没有旧脚本复制、旧生态运行时依赖或秘密泄露（源码/README/manifest 扫描、旧运行时标识检索和 git diff 检查通过）。

## 5. 当前依赖矩阵

### 编译依赖

`packtide`：

- `anyhow`
- `clap`
- `flate2`
- `ureq` + `rustls`

`systide`：

- `anyhow`
- `clap`
- `quick-xml`
- `ureq` + `rustls`

### 运行时依赖

基础复刻流程：

- `fzf`
- `pacman`
- `paru` 或 `yay`
- `sudo`（实际系统事务）

按功能可选：

- `flatpak`
- `reflector`
- `quicksave`
- `findmnt`
- `grub-mkconfig`
- `checkupdates`
- `pkill`
- `flock`
- `timedatectl`
- `date`

### 依赖剥离决策

- [x] 当前不把 fzf 换成 Ratatui；先保持第一轮行为一致。
- [ ] 在行为测试锁定后，再单独评估 Ratatui 迁移。
- [x] 不用 curl/gzip/Python 替换已经稳定的 Rust `ureq`/`flate2`/`quick-xml`（当前实现继续使用 Rust HTTP、gzip 和 XML 依赖）。
- [x] 将可选系统命令按子命令能力分级，而不是全部打成基础依赖（入口按能力使用 `require_command_for`/`command_exists`，README 及依赖矩阵列出 Flatpak、reflector、quicksave、GRUB、checkupdates、Waybar 等可选命令）。

## 6. 完成定义

本路线图不能以“代码拆开了”作为完成标准。每个阶段必须同时满足：

- 行为没有回归；
- 真实用户路径已执行；
- 高风险动作有明确确认和失败边界；
- 对应测试覆盖真实改变的路径；
- `main.rs` 和模块规模达到目标；
- 依赖说明与实际探测一致；
- 当前阶段的残余风险写入变更记录。

最终完成前，不应声称“已完成全部优化”，只能按复选框报告已完成项目和剩余项目。

参考代码快照仍位于仓库外的 `../independent-linux-tool-reference-20260924/`，不参与构建。
