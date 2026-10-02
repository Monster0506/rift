use super::*;

pub(super) mod common;

#[cfg(feature = "treesitter")]
mod bench_harness;
mod buffer_nav_tests;
mod display_map_tests;
mod editor_lsp_tests;
mod editor_paste_tests;
mod explorer_preview_tests;
mod ghost_cut_tests;
mod motion_tests;
mod plugin_highlight_tests;
mod plugin_settings_tests;
mod region_bank_ops_tests;
mod region_bank_selection_tests;
mod scroll_blit_tests;
mod surround_tests;
#[cfg(feature = "treesitter")]
mod syntax_treesitter_tests;
mod window_split_tests;
