# UI 文案与 i18n 迁移清单

当前资源格式：编译期内嵌 UTF-8 `key=value` 文件，不需要运行时依赖。

```text
crates/packtide/locales/{zh,en}.txt
crates/systide/locales/{zh,en}.txt
```

locale 选择规则：显式 `PACKTIDE_UI_LANG=zh|en` 优先；`auto`、未设置或未知值时按
`LC_ALL`、`LC_MESSAGES`、`LANG` 顺序寻找 `zh-*`/`en-*`；没有支持的 locale 时回退 English。
`systide --ui-lang=auto|zh|en` 保持 CLI 显式选择优先于环境 locale。

## Packtide

| 范围 | 当前源文件 | 资源 key 前缀 | 状态/阶段 |
| --- | --- | --- | --- |
| install/remove picker 标题、prompt、actions、refresh、helper、cancel | `src/ui/fzf.rs`, `src/install.rs`, `src/remove.rs` | `install.*`, `remove.*`, `picker.*`, `selection.*` | 部分迁移，阶段 1 |
| source labels、installed badge | `src/ui/colors.rs`, `src/ui/rows.rs`, `src/sources/flatpak.rs` | `source.*`, `package.*` | labels 仍部分代码固定，阶段 3/4 |
| preview summary、empty/error、metadata | `src/ui/preview.rs` | `preview.*` | summary/empty/error 已迁移；field labels来自包管理器，阶段 1 |
| check-updates prompt/header/refresh/empty/no-selection | `src/check_updates.rs` | `updates.*`, `picker.*`, `selection.*` | 未迁移，阶段 4 |
| update row source labels | `src/model.rs` | `source.*`, `updates.*` | 颜色/标签仍在代码，阶段 4 |
| downgrade prompt/header/warnings/transaction status | `src/downgrade.rs` | `downgrade.*`, `picker.*`, `transaction.*` | 未迁移，阶段 5 |
| mirror update progress/fallback/rollback diagnostics | `src/mirror_update.rs`, `src/mirror_update/transaction*.rs` | `mirror.*`, `error.*` | 未迁移，阶段 7 |
| package transaction summaries/errors | `src/transaction.rs`, `src/app.rs`, `src/commands.rs` | `transaction.*`, `error.*` | 未迁移，阶段 7 |
| source/cache/preview validation errors | `src/sources*.rs`, `src/ui/preview.rs` | `error.*`, `preview.*` | user-visible diagnostics remain English, phase-specific migration later |

## Systide

| 范围 | 当前源文件 | 资源 key 前缀 | 状态/阶段 |
| --- | --- | --- | --- |
| intro, ordinary flow, log labels, update steps | `src/messages.rs`, `src/main.rs`, `src/flow.rs` | `systide.*`, `flow.*`, `log.*` | existing messages moved to locale files; intro steps/log tags still in Rust, phase 6 |
| list picker prompt/header/refresh/empty | `src/ui.rs` | `systide.list.*`, `picker.*` | 未迁移，阶段 4 |
| news title/count/date/urgent/link fallback/fetch errors | `src/news.rs` | `news.*` | 部分消息在 `messages.rs`; count/title and fallback remain, 阶段 6 |
| mirror age/confirmation/skip/errors | `src/mirror.rs` | `mirror.*` | 未完整迁移，阶段 6 |
| snapshot/keyring/upgrade/partial failure | `src/snapshot.rs`, `src/update.rs` | `snapshot.*`, `update.*` | update messages partly localized; snapshot/errors/structured partial status remain, 阶段 6 |
| Flatpak/GRUB/Waybar finish results | `src/finish.rs` | `finish.*` | 未迁移，阶段 6 |
| CLI validation and executable failures | `src/cli.rs`, `src/main.rs`, `src/operations.rs` | `error.*` | technical diagnostics remain English; classify human-facing vs diagnostic before migrating |

## Rules

- Every human-facing string is zh/en paired before that UI stage is considered done.
- Technical diagnostics may remain stable English only when they are command names, paths, exit status,
  or error context; surrounding explanatory text uses locale resources.
- Never localize machine fields, package IDs, command argv, debug timing keys, or cache formats.
- Key lookup must return a visible missing-key marker; do not silently emit empty strings.
- Interpolation values are passed separately. Do not concatenate partial localized sentences in business code.
