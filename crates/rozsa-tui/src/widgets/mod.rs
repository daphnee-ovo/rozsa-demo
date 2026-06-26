// widgets/ — reusable UI atoms
//
// Internal Framework:
// widgets/
// ├── tab_bar     横向 tab 条
// └── hints_bar   底部键位提示

pub mod hints_bar;
pub mod tab_bar;

pub use hints_bar::{render_hints_bar, HintItem};
pub use tab_bar::{render_tab_bar, TabBarState};
