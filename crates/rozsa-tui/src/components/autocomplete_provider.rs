// components/autocomplete_provider.rs — 补全 Provider 架构
//
// Internal Framework:
// autocomplete_provider.rs
// └── AutocompleteProvider  trait (补全数据源接口)
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)

// components/autocomplete_provider.rs — 自动补全 Provider 接口（预留本地扩展点）
//
// Internal Framework:
// autocomplete_provider.rs
// └── AutocompleteProvider    pub trait 补全 provider 接口
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)

use crate::protocol::NativeAutocompleteItem;

/// 自动补全 Provider 接口（预留本地 provider 扩展点）
pub trait AutocompleteProvider: Send + Sync {
    fn handles_prefix(&self, prefix: &str) -> bool;
    fn complete(&self, text: &str, cursor: usize) -> Vec<NativeAutocompleteItem>;
}
