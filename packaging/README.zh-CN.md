[English](README.md) | 简体中文

# 打包

发布内容包含两个二进制文件：`packtide` 和 `systide`。支持的安装路径如下：

在 AUR 恢复注册前，所有发行版都直接运行仓库根目录的 `install.sh`。它会选择 GNU 或 musl 归档，在支持的
发行版上补全 `fzf` 和权限依赖，并在替换旧的 `pac`/`sysup` 类命令前询问。安装后同时提供
`ptd` 和 `suu` 两个正式缩写。

* 从项目发布页面下载已发布的 Linux tarball，将两个二进制文件复制到 `PATH` 中的目录，并把 `man/` 和 `completions/` 中的文件安装到对应的系统目录。
* Arch Linux：从 checkout 开始，在 `packaging/packtide/` 中运行 `makepkg -si`。该配方目前仍是 checkout-local；提交官方包仓库时应切换到带标签的源码归档。
* 从 checkout 安装：运行 `cargo install --path crates/packtide --locked` 和 `cargo install --path crates/systide --locked`。这会安装二进制文件；shell 补全和 man 页面仍可从本目录取得。

[`发布工作流`](https://github.com/muwenyan521/packtide/actions/workflows/release.yml) 为 `x86_64-unknown-linux-gnu` 构建 tarball，并包含中英文 man 页面和 SHA-256 sidecar 文件。运行时依赖和后端支持记录在仓库 README 中。
