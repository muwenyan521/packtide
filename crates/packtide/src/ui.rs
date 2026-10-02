mod colors;
mod fzf;
mod preview;
mod rows;

pub(crate) const NO_SELECTION: &str = "No packages selected.";
pub(crate) const COMMON_FZF_LAYOUT_ARGS: &[&str] = &[
    "--ansi",
    "--no-wrap",
    "--no-hscroll",
    "--ellipsis=...",
    "--layout=reverse",
    "--border",
    "--height=95%",
    "--tiebreak=index",
    "--bind",
    "change:first",
];

pub(crate) use colors::{source_color, strip_ansi};
pub(crate) use fzf::{
    classify_picker_status, picker_columns, picker_header, select_install_catalog_streaming,
    select_rows, wrap_shortcut_line,
};
pub(crate) use preview::{preview_command, shell_quote};
pub(crate) use rows::{
    PackageListMode, parse_package_row, render_package_rows, write_install_catalog,
};
