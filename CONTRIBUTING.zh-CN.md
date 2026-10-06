[English](CONTRIBUTING.md) | 简体中文

# 参与贡献

保持改动聚焦，并说明用户可见行为、受影响的后端和失败边界。修改子系统前先阅读对应作用域的 `AGENTS.md`。

## 本地检查

使用能覆盖改动的最小测试套件；在发布或跨模块交接前，再运行工作区检查：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release --locked
```

后端改动必须测试精确 argv、stderr/状态码、作用域、locale 和权限。UI 改动必须执行真实 TUI 预览/取消场景，不能只做行快照。包事务要使用一次性容器或虚拟机；不要针对主机包数据库测试安装或卸载。参见[支持契约](docs/package-manager-support.md)中的矩阵命令。

对每项声明报告调用、源代码提交、可观察结果和保留的工件。区分单元/伪造测试与真实后端验证，并如实报告不可用通道。公开报告不得包含凭据、个人数据或机器专属路径。

## 源代码与许可证

本项目按 [MPL-2.0](LICENSE) 分发。贡献内容必须兼容该许可证。添加代码前保留声明，并确认第三方代码及其许可证。历史参考项目不代表可以复制其代码或文字；参见 [NOTICE.md](NOTICE.md)。能力或失败行为变化时，更新支持契约和用户文档。
