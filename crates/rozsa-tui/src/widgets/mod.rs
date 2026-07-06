// widgets/ — reusable UI atoms
//
// Internal Framework:
// widgets/
// ├── tab_bar     横向 tab 条
// └── hints_bar   底部键位提示

pub mod hints_bar;
pub mod tab_bar;

pub use hints_bar::{HintItem, render_hints_bar};
pub use tab_bar::{TabBarState, render_tab_bar};
