use crate::action::{Action, UndoTreeAction};
use crate::component::{Component, EventResult};
use crate::history::EditSeq;
use crate::history::UndoTree;
use crate::key::Key;
use crate::message::{AppMessage, UndoTreeMessage};
use crate::select_view::SelectView;
use crate::state::UserSettings;
use crate::undotree_view::render_tree;
use std::any::Any;

pub struct UndoTreeComponent {
    pub view: SelectView,
}

impl Component for UndoTreeComponent {
    fn handle_input(&mut self, key: Key) -> EventResult {
        self.view.handle_input(key)
    }

    fn handle_action(&mut self, action: &Action) -> EventResult {
        let undotree_action = match action {
            Action::UndoTree(a) => a,
            _ => return EventResult::Ignored,
        };

        match undotree_action {
            UndoTreeAction::Close => {
                EventResult::Message(AppMessage::UndoTree(UndoTreeMessage::Cancel))
            }
            UndoTreeAction::Down => self.view.move_selection_down(),
            UndoTreeAction::Up => self.view.move_selection_up(),
            UndoTreeAction::Select => {
                if let Some(_idx) = self.view.selected_line() {
                    // We need to re-trigger the on_select callback logic here for now
                    // Since SelectView callbacks are closures, we can't easily trigger them from outside
                    // without exposing them or simulating input.
                    // Instead, simpler approach: Simulate Key::Enter
                    self.view.handle_input(Key::Enter)
                } else {
                    EventResult::Consumed
                }
            }
        }
    }

    fn render(&mut self, layer: &mut crate::layer::Layer) {
        self.view.render(layer)
    }

    fn get_context(&self) -> crate::keymap::KeyContext {
        crate::keymap::KeyContext::UndoTree
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

pub fn create_undo_tree_component(
    undo_tree: &UndoTree,
    settings: &UserSettings,
) -> (Box<dyn Component>, Option<AppMessage>) {
    let (lines, seqs, cursor) = render_tree(undo_tree);

    let selectable: Vec<bool> = seqs.iter().map(|&s| s != EditSeq::MAX).collect();

    let mut view = SelectView::new()
        .with_left_width(50)
        .with_colors(settings.editor_fg, settings.editor_bg);

    view.set_left_content(lines);

    view.set_selected_line(Some(cursor));
    let view = view.with_selectable(selectable);

    // Create initial preview message
    let initial_message = if let Some(&seq) = seqs.get(cursor) {
        if seq != EditSeq::MAX {
            Some(AppMessage::UndoTree(UndoTreeMessage::Preview(seq as usize)))
        } else {
            None
        }
    } else {
        None
    };

    let sequences = seqs;
    let seqs_select = sequences.clone();
    let seqs_change = sequences.clone();

    let view = view
        .on_select(move |idx| {
            if let Some(&seq) = seqs_select.get(idx) {
                if seq != EditSeq::MAX {
                    return EventResult::Message(AppMessage::UndoTree(UndoTreeMessage::Goto(
                        seq as usize,
                    )));
                }
            }
            EventResult::Consumed
        })
        .on_change(move |idx| {
            if let Some(&seq) = seqs_change.get(idx) {
                if seq != EditSeq::MAX {
                    return EventResult::Message(AppMessage::UndoTree(UndoTreeMessage::Preview(
                        seq as usize,
                    )));
                }
            }
            EventResult::Consumed
        })
        .on_cancel(|| EventResult::Message(AppMessage::UndoTree(UndoTreeMessage::Cancel)));

    (Box::new(UndoTreeComponent { view }), initial_message)
}
