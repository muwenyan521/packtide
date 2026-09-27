# packtide / systide UI 美化计划

> 状态：阶段 0–1 实现进行中；阶段 2 仍待补齐真实交互与窄终端证据。
>
> 目标：在保留 fzf、现有键位、preview、事务安全边界和产品职责划分的前提下，
> 真正提升界面的质感、层次、可读性和反馈感。第一优先级是常用的
> `packtide install`，不是把几套页面简单换成同一组颜色。

## 研究结论

### 当前实现事实

- `packtide install/remove` 的 fzf 配置主要位于
  `crates/packtide/src/ui/fzf.rs`，行渲染位于 `crates/packtide/src/ui/rows.rs`，
  source 颜色位于 `crates/packtide/src/ui/colors.rs`。
- `packtide check-updates` 自己维护 prompt/header 和更新行渲染，位于
  `crates/packtide/src/check_updates.rs` 与 `crates/packtide/src/model.rs`。
- `packtide downgrade` 自己维护另一套行格式和 fzf 参数，位于
  `crates/packtide/src/downgrade.rs`。
- `systide --list` 有独立的 fzf 参数、source 颜色和行渲染，位于
  `crates/systide/src/ui.rs`。
- `systide` 普通流程的 intro、步骤、日志标签和双语文案位于
  `crates/systide/src/messages.rs`、`main.rs`、`mirror.rs`、`update.rs`、
  `finish.rs`；新闻输出位于 `crates/systide/src/news.rs`。
- 当前包列表已经具备 source 色、CJK 宽度计算和绿色 `✔ [已安装]`，但各页面
  的颜色 token、列 schema、prompt、header 和 helper 状态没有统一来源。
- 当前长包名、长版本主要通过 padding 处理，不能保证在窄终端中稳定省略；
  Flatpak 行与 Pacman/AUR 行的字段结构也不完全相同。
- `strip_ansi` 是简化 CSI 清理器，不应在未增强解析前引入复杂 OSC/256 色到
  可被选中的数据行中。

### 原项目视觉依据

只读参考资料位于仓库外，不参与构建：

- 原 pac 安装行、header、preview：历史参考 clone/bundle 中的 `pacman/pac`。
- 原降级页面：
  `/home/wangxianming/dev/independent-linux-tool-reference-20260924/snapshot/pacman/pacd`
  的 fzf 配置和 `Using downgrade & helper` header。
- 原更新列表：
  `/home/wangxianming/dev/independent-linux-tool-reference-20260924/snapshot/pacman/checkallupdates`
  的 `📦 待更新项目 > `、`⏳ 正在拉取最新数据... > `、header 和 reload。
- 原系统更新流程：
  `/home/wangxianming/dev/independent-linux-tool-reference-20260924/snapshot/system/sysup`
  的 intro、阶段消息、新闻标题、日志标签、镜像提示和 partial failure 警告。

### 视觉判断

当前主要问题不是颜色数量不足，而是视觉系统分散：

```text
install/remove       ui/fzf.rs + ui/rows.rs
check-updates        check_updates.rs + model.rs
downgrade            downgrade.rs
systide --list       systide/ui.rs
systide 普通流程     messages.rs + news.rs
```

美化应让用户能明显感知：页面更有层次，信息更容易扫读，当前状态更明确，
预览更像一个完整的信息面板，刷新/加载/成功/失败有清晰反馈，空白和拥挤得到
控制。统一 token 只是底层手段，不是交付目标。

## 真正的美化方向

### A. `packtide install` 的视觉升级

- **建立“品牌入口”**：header 不再只是键位说明，增加简短而克制的页面标题/状态
  识别，例如 `PACKTIDE · 安装软件包`，右侧保留 `Using paru`/`Using yay`；键位
  说明降为次要色，避免整行文字同等抢夺注意力。
- **做出信息层级**：source 使用稳定色相，包名使用高亮主色，版本使用较低亮度，
  已安装 badge 使用绿色粗体；不要让 source、名字、版本、状态全部同样醒目。
- **改善行的节奏**：固定列之间加入可感知但不浪费空间的留白，统一 `source → name
  → version → state` 的阅读流；AUR 行用明确的 `AUR` 标签和版本占位，不让 `-`
  看起来像缺失数据。
- **突出可行动状态**：首次加载、追加 AUR、正在刷新、无结果、取消和事务准备分别
  使用不同的 prompt/status 文案；刷新时显示短暂的状态提示，而不是只改变一段普通文字。
- **强化 preview**：把 preview 做成“包详情面板”，字段名是统一的强调色，值保持
  清晰正文；顶部增加包名/source/version 摘要，失败时用醒目的错误行和下一步提示，
  空详情时显示明确占位，不让面板看起来像坏了。
- **处理窄屏**：优先保留包名和版本的可读性，source 和辅助字段可以降级/省略；超长
  内容显示省略号而不是挤压其它列或换行破坏列表节奏。
- **增加选择反馈**：检查当前 fzf pointer、marker、selected 状态的对比度，保证鼠标
  或键盘移动时当前行一眼可见；多选状态要与已安装状态形成不同视觉语义。

### B. 其它 packtide 页面不只是套模板

- `remove`：整体更偏“危险操作”但不使用大面积红色；卸载动作、Flatpak 应用、AUR
  包和不可逆提示分层显示，`Alt-C` 作为显眼但克制的快捷操作。
- `check-updates`：把更新条目从普通包列表提升为“待处理清单”，source 标签更轻，
  更新名称和版本变化更突出，列表顶部显示更新数量/状态。
- `downgrade`：强调风险和版本选择，当前版本、目标版本、来源和危险提示必须有明显
  层级；警告应像确认门，不要和普通 header 混成一行。
- Flatpak：专门处理应用名、application ID、origin 三层信息，不强行塞进 Pacman 的
  版本列，避免看起来像格式错乱。

### C. systide 的产品感

- intro 做成有节奏的“系统维护流程”：标题、目标摘要、将要执行的步骤、权限状态
  分层显示，而不是连续的脚本日志。
- 新闻区分标题、数量、日期、urgent 和链接；普通新闻安静，urgent 有明确警示，
  OSC-8 链接保留但不让转义影响纯文本阅读。
- 更新阶段使用稳定的步骤指示、成功/警告/失败徽记；partial failure 要成为一个
  清晰的事故状态块，而不是一条挤满信息的长句。
- 镜像、快照、keyring、包升级、Flatpak、GRUB、Waybar 各自有简短状态标题，让用户
  能知道系统正在做什么以及下一步是什么。
- `packtide sysup` 只保留兼容桥；美化和系统更新反馈只在 `systide` 实现一次。

## 设计原则与不变约束

- `packtide install` 是第一优先级；其它页面在 install 视觉规则稳定后再收敛。
- 保留 fzf，不迁移 Ratatui，不把页面改造成全新交互产品。
- 保留现有 `Tab`、`Enter`、`Esc`、`Ctrl-R`、`Alt-J`、`Alt-K`、`Alt-C` 语义。
- 保留结构化 preview argv、普通用户查询 preview、真实事务才提权的边界。
- 视觉 ANSI 不能改变 tab 数据列、`--nth`、`--id-nth`、包名解析或事务 argv。
- 原始值与显示值分离：可以显示省略号，但不能把省略后的名字传给 preview 或事务。
- 默认优先 ANSI 16 色和高对比度；只使用少量常用语义符号（`✔`、`!`、`·`、`>`），
  不使用装饰性 emoji，不把界面做成图标墙。
- 不在页面中引入无意义动画；状态变化只在真实刷新、加载、成功、失败、取消时反馈。
- 本轮按用户要求不把截图、像素 diff 或视觉 QA 作为完成门槛；每个阶段仍需做代码
  快检、目标单测和一次真实 CLI/TUI 启动 smoke，避免把“计划中的美化”误当成已生效。

### i18n 约束

- 美化不再继续向业务代码散落中文/英文字符串；所有用户可见文案必须进入集中语言资源。
- 首批保持当前 `zh`/`en` 两种语言，并保留 `auto` locale 选择；资源格式优先选择简单、可审查、
  无运行时网络依赖的本地文件，例如 `locales/zh.toml` 与 `locales/en.toml`。
- packtide 和 systide 都必须接入 i18n，覆盖 picker header、prompt、刷新状态、空选择、错误、
  事务摘要、preview 错误、check-updates、downgrade 和系统更新阶段文案。
- key 按页面和状态分组，例如 `install.header`、`install.prompt`、`picker.refreshing`、
  `transaction.cancelled`、`systide.news_title`；同一语义只保留一个 key。
- 插值值由代码传入，禁止在业务层拼接半句中文再交给翻译层；缺失 key 不能静默显示空字符串。
- 新增文案先补齐 zh/en 两份，再进入 UI 美化；不接受只完成中文、英文之后补的顺序。
- 语言资源不改变机器可解析字段、事务 argv、debug timing 或错误退出码。

## 0. 视觉基线与设计决策

- [x] 固定研究环境和原项目参考路径；不要求生成截图/像素 diff。
- [x] 记录当前实现、原 pac/syup 文案与 fzf 参数差异；缺少原安装入口参考的部分明确标为未知。
- [x] 建立 UI token 表：source、status、helper、action、prompt、preview、info、success、warning、error、reset。（本阶段映射见下表。）
- [x] 决定 source token 的统一映射：Pacman、AUR、Flatpak 在包页、更新页、systide list 中保持同一语义色。（包页、更新行和 systide list 统一为 Pacman 蓝 34、AUR 洋红 35、Flatpak 青 36。）
- [x] 决定符号策略：只使用 `✔`、`!`、`·`、`>` 等常用符号；原 pac 的 emoji prompt 只作为参考，不直接照搬。（不使用 emoji。）
- [x] 决定颜色策略：先使用 ANSI 16 色和粗体，避免未经验证的 256 色/RGB 依赖。
- [x] 决定显示列与数据列的分离方式，确保省略号不进入 parser、preview 或事务参数。（保留原始 tab 行；fzf `--no-wrap --ellipsis=...` 仅负责终端显示截断，选择输出仍是原始行。）
- [x] 将设计决策写入本文件和独立 token/迁移文档，避免后续页面各自解释。（本文件 token 表与 `.omo/docs/UI_I18N_INVENTORY.md`。）
- [x] 盘点 packtide/systide 全部用户可见字符串，按页面、状态和语言 key 建立迁移表。（见 inventory；未迁移项保留对应阶段。）
- [x] 选定集中资源目录和加载 API，确定 `auto/zh/en` 的 locale 归一化规则与 fallback 链。（packtide locale loader 已覆盖显式值、auto、LC_ALL/LC_MESSAGES/LANG、zh/en 区域后缀。）
- [x] 先迁移一条 install picker 文案链做最小 i18n 骨架，再扩展其它页面。（install/remove picker、badge、preview empty/error 已接入。）

### 首版视觉 Token

| Token | ANSI/呈现 | 用途 |
| --- | --- | --- |
| `source.pacman` | 蓝 `34` | 官方仓库来源 |
| `source.aur` | 洋红 `35` | AUR 来源 |
| `source.flatpak` | 青 `36` | Flatpak 来源 |
| `text.primary` | 粗体 `1` | 包名/主要操作对象 |
| `text.secondary` | 暗色 `2` | 版本、origin、辅助信息 |
| `status.installed` | 绿 `32` + `✔` | 已安装状态 |
| `status.helper` | 黄 `33` | `Using paru/yay` |
| `status.warning` | 黄 `33` + `!` | 可恢复警告 |
| `status.error` | 红 `31` + `!` | 失败与危险状态 |
| `text.info` | 青 `36` | preview 字段标题/页面识别 |
| `reset` | `0` | 所有 ANSI 样式结束 |

### i18n 文案迁移范围

| 页面/状态 | 当前主要位置 | 资源 key 前缀 | 阶段 |
| --- | --- | --- | --- |
| install/remove picker | `ui/fzf.rs`, `install.rs`, `remove.rs` | `install.*`, `remove.*`, `picker.*` | 1 |
| installed badge / source labels | `ui/rows.rs`, `ui/colors.rs` | `package.*`, `source.*` | 1/后续 |
| preview empty/error | `ui/preview.rs` | `preview.*` | 1 |
| check-updates | `check_updates.rs`, `model.rs` | `updates.*`, `picker.*` | 4 |
| downgrade | `downgrade.rs` | `downgrade.*`, `picker.*` | 5 |
| systide flow/news/mirror/update | `messages.rs`, `news.rs`, `mirror.rs`, `update.rs` | `systide.*`, `news.*`, `mirror.*` | 6 |
| transaction summaries/errors | `transaction.rs`, `mirror_update.rs`, `app.rs` | `transaction.*`, `error.*` | 7 |

## 1. `packtide install` 主页面

### Header、prompt 与状态

- [x] 重做 header 层次：增加 `PACKTIDE · 安装软件包` 页面识别，键位说明降为次要色，
  `Using paru/yay` 作为右侧状态而不是普通句尾。
- [x] 明确 `paru`、`yay` 的 helper 状态色，且与 source 色、安装状态色完全区分。（helper 黄、source 蓝/洋红/青、installed 绿。）
- [x] 把 prompt 分成“可搜索状态”和“刷新状态”，刷新时使用短状态文案或 `!` 状态符号，完成后
  恢复简洁的 `待安装项目 > `；不引入装饰性 emoji。
- [ ] 为无结果、取消、source 失败和 writer 失败设计不同的反馈层级，不再共用一条普通句子。
- [x] 让 header/prompt 在窄终端自动缩短次要提示，不牺牲页面标题和当前状态。（按 `COLUMNS` 选择单行/简化操作提示，标题与 helper 保留。）
- [x] header/prompt 的中文和英文都从语言资源读取；刷新 bind 中不得继续硬编码中文。

### 行渲染

- [x] 固定视觉层级：source 低亮、package name 高亮、version 次亮、installed badge 绿色粗体。
- [x] 给当前 fzf pointer/marker 和 Tab 多选状态增加更明确的对比度，避免“当前行”和“已安装”混淆。（保留 fzf marker/track 契约，已安装独立绿色 token；真实三尺寸 smoke 无残留进程。）
- [ ] 保留 source 16 列、包名 35 列、版本 20 列的行为基线，按 Unicode 显示宽度计算。
- [x] 保留绿色 `✔ [已安装]`，统一其前后空格、颜色和与版本列的间距。（badge zh/en 资源，remove 不显示。）
- [ ] 增加长包名、长版本和 CJK 显示省略策略；优先保留包名/版本，省略 source 或辅助字段；
  只省略显示文本，不改变原始值。
- [x] 明确 AUR 行的 `aur` source 色、无版本显示和已安装状态层级。（统一用洋红来源、次级版本占位、绿色已安装 token。）
- [x] 保留 tab 数据分隔和 `--nth/--id-nth` 解析契约。（行渲染只增加 ANSI，不改字段或原始选择行。）

### Preview

- [x] 保留当前结构化 argv 和普通用户权限执行。（preview 仍使用结构化 `Command` argv 和普通用户执行。）
- [x] 保持原 pac 的 metadata label 粗体青色语义。（现有 label/value 着色测试继续通过。）
- [x] 将 preview 做成信息面板：顶部显示包名/source/version 摘要，正文采用 label/value 分层，
  空详情显示占位，查询失败显示红色错误和可行动提示；不改变包详情正文。
- [ ] 显式验证 helper 输出的颜色、section header 和 ANSI 清理，不让复杂转义污染选中行解析。（已补 `--color=always`；复杂 ANSI/OSC 专项测试仍待。）
- [ ] 为 preview 输出增加窄窗口长值策略，避免覆盖 header 或产生不可读横向滚动。
- [x] preview 失败、空详情和参数错误文案进入语言资源，并保留 source/helper/package 插值。（empty/failed/source labels 已接入。）

### 交互

- [x] 验证 Tab 多选、Enter 安装、Esc 取消、Ctrl-R 刷新、Alt-J/K 导航。（现有 fake/PTY 路径保持；新增 pointer/marker 参数测试。）
- [ ] 复现并修复鼠标点选偏移，重点检查 `--height=95%`、reverse layout、border、pointer/marker 组合。
- [ ] 验证 Ctrl-R 后 query、当前选择、preview 和候选身份保持。

## 2. install 实现后的最小验收

- [x] 运行一次真实 `packtide install`，确认页面能启动、候选可搜索、preview 可打开、
  Esc 可退出；不制作截图或像素报告。
- [x] 用现有行渲染单测覆盖 source 色、已安装 badge、CJK 宽度、超长名称/版本的显示值。（新增 install/remove emphasis 隔离与 locale tests；超长截断仍待补。）
- [x] 用 fake fzf/命令测试确认 picker argv、取消/接受/preview/事务 argv 没有变化。（新增 pointer/marker/header/prompt 断言；键位由 fzf 原生绑定保持。）
- [x] 对 80×24、120×40、160×50 只做一次文本宽度和溢出快检，发现明显错位再修，不建立视觉 QA 门禁。（当前 release install smoke 均成功启动并 Esc 退出。）

## 3. `packtide remove`

- [ ] 复用 install 的 source/helper/action token，但不显示误导性的 installed badge。
- [ ] 统一 Pacman/AUR/Flatpak source 颜色和字段层级。
- [ ] 统一 Flatpak 应用名、application ID、origin 的列宽和 preview 层级。
- [ ] 统一 remove header、prompt、刷新中、取消、空选择和失败文案。
- [ ] remove 与 install 共用 picker/action 状态 key，不复制一套近似中文。
- [ ] 验证 Tab、Enter、Esc、Alt-C、Ctrl-R 和最终事务 argv。

## 4. `packtide check-updates` 与 `systide --list`

- [ ] 统一 Pacman/AUR/Flatpak source token，解决当前不同页面使用不同 ANSI 映射的问题。
- [ ] 决定并实现简洁 prompt/status 符号（优先 `✔`、`!`、`>`），不直接复制原版 emoji。
- [ ] 统一 `--info`、`--ellipsis`、`--no-wrap`、header、prompt 和 refresh 状态。
- [ ] 保留 reload-sync、cache 行为、source 顺序、去重和 `packtide sysup → systide` bridge。
- [ ] 验证更新列表顺序、取消、Enter、Ctrl-R 后选择状态和错误显示。
- [ ] 非交互输出与交互列表保持相同 source 颜色语义；若终端不支持 ANSI，保持可读纯文本。
- [ ] check-updates 与 systide list 的 header、prompt、刷新状态和空列表文案全部迁入 zh/en 资源；
  `--ui-lang`/locale 选择必须真实影响列表页面。

## 5. `packtide downgrade`

- [ ] 给 source 加入统一颜色 token。
- [ ] 统一 source/name/version 列宽和长版本显示策略。
- [ ] 统一 header、prompt、preview 和 helper 状态。
- [ ] 保留 downgrade 的危险警告、确认边界和取消不执行事务。
- [ ] 验证 CJK、超长版本、Tab、Enter、Esc、preview 和真实事务参数。
- [ ] downgrade 的动作、危险警告、取消和事务摘要从集中语言资源读取，保留 helper 插值。

## 6. `systide` 普通流程与新闻

- [ ] 重做 intro：标题、目标摘要、步骤列表、权限状态分层；用短标题和状态符号减少脚本感。
- [ ] 集中 intro、step、info/success/warn/error 标签的 token 和文案，补齐红色 error 标签。
- [ ] 补齐原 sysup 的准备、权限请求、抓取、强制、取消、安全退出等阶段反馈，并让当前步骤
  有明确的进行中/完成/跳过状态。
- [ ] 新闻标题显示真实数量，例如“最近 N 条新闻”，保留 urgent 红色层级；普通新闻降低亮度，
  链接和日期形成稳定的次级信息行。
- [ ] 新闻日期按 Unicode/终端显示宽度安全裁剪，不按固定字符索引误切。
- [ ] 保留 OSC-8 链接，并提供终端不支持时的纯文本 URL fallback。
- [ ] 镜像提示显示具体年龄，区分新鲜、过旧、无时间标记、无脚本和跳过。
- [ ] partial failure 改为分层醒目输出：红色标题、数据库已更新、禁止部分升级、需要重新运行
  四个独立信息块，避免压缩为一条长句。
- [ ] 窄终端下 intro、新闻、确认、警告和取消文案不重叠、不溢出。
- [ ] 中英文文案的 prompt、header、阶段标签、错误和取消语义保持一致。
- [ ] 将现有 `messages.rs` key 表迁移为集中语言资源；代码只保留 key、插值和状态逻辑。
- [ ] 把 `main.rs`、`mirror.rs`、`snapshot.rs`、`news.rs`、`ui.rs` 的硬编码文案全部纳入资源盘点，
  确保 `--ui-lang=en` 不会在列表页、刷新提示或可选依赖错误中混入中文。

## 7. 跨页面回归与交付

- [ ] 各页面做一次真实 CLI/TUI smoke；不要求截图、像素 diff 或外部视觉评审。
- [ ] 统一检查 CJK、超长字段、窄终端文本宽度和关键键位；只记录可复现的功能/布局错误。
- [ ] 保留 fake-command、parser、preview、事务、退出码和取消测试。
- [ ] UI 改动完成后运行最窄必要检查：fmt、目标测试、clippy；跨 crate token 改动后再跑 workspace test/build。
- [ ] 重新采集 release hash、实际运行结果和必要的文本宽度检查结果。
- [ ] 更新 README 和 REFACTOR_ROADMAP，仅勾选有代码、测试和真实视觉证据的项目。
- [ ] README 说明支持的 locale、`auto` 规则和资源目录；新增语言只需增加资源文件，不改业务流程。
- [ ] 明确未验证项：终端主题差异、字体差异、鼠标协议差异、OSC-8 支持差异、缺失的原 pac 安装入口参考；这些不作为本轮完成阻塞项。

## 风险清单

- [ ] ANSI 文案变化不能破坏 `parse_package_row`、`--nth`、`--id-nth`、preview 和 fake-fzf 精确断言。
- [ ] `strip_ansi` 在引入复杂 CSI/OSC/256 色前必须增强或限制输入范围。
- [ ] Flatpak 与 Pacman/AUR 的字段结构不同，不强行套用错误的版本列 schema。
- [ ] `down:55%` 在 80×24 下会压缩列表，任何 header/prompt 变更都必须重新做窄终端 QA。
- [ ] `packtide` 与 `systide` 不直接互相依赖；共享 token 时只能放入无业务依赖的纯终端层，或在两个 crate 保持同构本地 token。
- [ ] 原 pac 主安装入口完整参考不足时，不把 pacd/checkallupdates/sysup 的证据冒充 install 参考。

## 完成标准

- [ ] `packtide install` 的层次、留白、状态反馈、preview、prompt/header 和窄终端策略有代码与真实 smoke 证据。
- [ ] install 的美化不是只换颜色：至少包含 header/状态重构、行层级、preview 信息面板和超长字段策略中的三项。
- [ ] remove、check-updates、downgrade、systide list 与 install token 统一且没有行为回归。
- [ ] systide 普通流程/news 的文案、颜色、警告层级和阶段反馈完成真实 smoke 验证。
- [ ] workspace fmt、clippy、test、release build 通过，release hash 和文档同步。
- [ ] 未验证项和原 pac 参考缺口明确记录，不用“看起来更好”替代证据。
