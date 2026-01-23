use crate::component::{Component, EventResult};
use crate::history::EditSeq;
use crate::history::UndoTree;
use crate::select_view::SelectView;
use crate::state::UserSettings;
use crate::undotree_view::actions::{
    UndoTreeCancelAction, UndoTreeGotoAction, UndoTreePreviewAction,
};
use crate::undotree_view::render_tree;

pub fn create_undo_tree_component(
    undo_tree: &UndoTree,
    settings: &UserSettings,
) -> Box<dyn Component> {
    let (lines, seqs, cursor) = render_tree(undo_tree);

    let selectable: Vec<bool> = seqs.iter().map(|&s| s != EditSeq::MAX).collect();

    let mut view = SelectView::new()
        .with_left_width(50)
        .with_colors(settings.editor_fg, settings.editor_bg);

    view.set_left_content(lines);

    view.set_selected_line(Some(cursor));
    let view = view.with_selectable(selectable);

    let sequences = seqs;
    let seqs_select = sequences.clone();
    let seqs_change = sequences.clone();

    let view = view
        .on_select(move |idx| {
            if let Some(&seq) = seqs_select.get(idx) {
                if seq != EditSeq::MAX {
                    return EventResult::Action(Box::new(UndoTreeGotoAction { seq }));
                }
            }
            EventResult::Consumed
        })
        .on_change(move |idx| {
            if let Some(&seq) = seqs_change.get(idx) {
                if seq != EditSeq::MAX {
                    return EventResult::Action(Box::new(UndoTreePreviewAction { seq }));
                }
            }
            EventResult::Consumed
        })
        .on_cancel(|| EventResult::Action(Box::new(UndoTreeCancelAction)));

    Box::new(view)
}
