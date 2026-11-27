use super::super::split::window::WindowId;
use super::explorer::PendingExplorerPreview;
#[cfg(feature = "plugins")]
use crate::document::runtime::BufferKindId;
use crate::document::runtime::DocumentHandle;
use crate::document::DocumentId;
#[cfg(feature = "plugins")]
use crate::plugin::events::SaveHold;
use crate::time::Instant;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
#[cfg(feature = "plugins")]
use std::path::PathBuf;
use std::time::Duration;

pub(super) const SYNTAX_REPARSE_DEBOUNCE: Duration = Duration::from_millis(40);
pub(super) const SEARCH_REFRESH_DEBOUNCE: Duration = Duration::from_millis(40);
pub(super) const GIT_GUTTER_DEBOUNCE: Duration = Duration::from_millis(400);

pub(super) const KEY_SEQUENCE_TIMEOUT: Duration = Duration::from_secs(1);
pub(super) const FRAME_INTERVAL: Duration = Duration::from_nanos(16_666_667);

pub(super) struct Scheduler {
    pub(super) key_sequence_started_at: Option<Instant>,
    pub(super) deferred_keys: VecDeque<crate::key::Key>,
    pub(super) terminal_frame_dirty: bool,
    pub(super) last_frame_at: Option<Instant>,
    pub(super) syntax_reparse: Debounced<DocumentId>,
    pub(super) search_refresh: Debounced<()>,
    pub(super) git_gutter_diff: Debounced<DocumentId>,
    pub(super) explorer_preview: Option<PendingExplorerPreview>,
    pub(super) explorer_cursor_entry: Option<String>,
    pub(super) git_status_expand_all: HashMap<DocumentId, bool>,
    pub(super) git_log_expand_head: HashSet<DocumentId>,
    pub(super) quit_after_job: Option<usize>,
    pub(super) quit_after_job_window: Option<WindowId>,
    #[cfg(feature = "plugins")]
    pub(super) save_pre_in_progress: HashSet<DocumentId>,
    #[cfg(feature = "plugins")]
    pub(super) held_saves: HashMap<DocumentId, HeldSave>,
    #[cfg(feature = "lsp")]
    pub(super) goto_after_load: Option<(DocumentId, usize, usize)>,
    #[cfg(feature = "lsp")]
    pub(super) code_actions: Vec<serde_json::Value>,
}

#[cfg(feature = "plugins")]
pub(super) struct HeldSave {
    pub(super) hold: SaveHold,
    pub(super) handle: DocumentHandle,
    pub(super) kind: BufferKindId,
    pub(super) source_path: Option<PathBuf>,
    pub(super) path: Option<PathBuf>,
    pub(super) force: bool,
    pub(super) quit: Option<SaveQuit>,
}

pub(super) struct SaveQuit {
    pub(super) handle: DocumentHandle,
    pub(super) window: WindowId,
    pub(super) message: Option<String>,
}

impl Scheduler {
    pub(super) fn new() -> Self {
        Self {
            key_sequence_started_at: None,
            deferred_keys: VecDeque::new(),
            terminal_frame_dirty: false,
            last_frame_at: None,
            syntax_reparse: Debounced::new(SYNTAX_REPARSE_DEBOUNCE),
            search_refresh: Debounced::new(SEARCH_REFRESH_DEBOUNCE),
            git_gutter_diff: Debounced::new(GIT_GUTTER_DEBOUNCE),
            explorer_preview: None,
            explorer_cursor_entry: None,
            git_status_expand_all: HashMap::new(),
            git_log_expand_head: HashSet::new(),
            quit_after_job: None,
            quit_after_job_window: None,
            #[cfg(feature = "plugins")]
            save_pre_in_progress: HashSet::new(),
            #[cfg(feature = "plugins")]
            held_saves: HashMap::new(),
            #[cfg(feature = "lsp")]
            goto_after_load: None,
            #[cfg(feature = "lsp")]
            code_actions: Vec::new(),
        }
    }

    fn key_sequence_deadline(&self) -> Option<Instant> {
        self.key_sequence_started_at
            .map(|start| start + KEY_SEQUENCE_TIMEOUT)
    }

    pub(super) fn key_sequence_expired(&self, now: Instant) -> bool {
        self.key_sequence_deadline()
            .is_some_and(|deadline| deadline <= now)
    }

    pub(super) fn terminal_frame_delay(&self, now: Instant) -> Option<Duration> {
        self.terminal_frame_dirty.then(|| {
            self.last_frame_at.map_or(Duration::ZERO, |last| {
                FRAME_INTERVAL.saturating_sub(now.saturating_duration_since(last))
            })
        })
    }

    pub(super) fn next_deadline(
        &self,
        active_document: DocumentId,
        now: Instant,
    ) -> Option<Instant> {
        [
            self.syntax_reparse.next_deadline(),
            self.search_refresh.next_deadline(),
            self.git_gutter_diff.deadline(active_document),
            self.key_sequence_deadline(),
            self.terminal_frame_delay(now).map(|delay| now + delay),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    pub(super) fn forget_document(&mut self, id: DocumentId) {
        self.syntax_reparse.cancel(id);
        self.git_gutter_diff.cancel(id);
        self.git_status_expand_all.remove(&id);
        self.git_log_expand_head.remove(&id);
        #[cfg(feature = "plugins")]
        self.held_saves.remove(&id);
        #[cfg(feature = "lsp")]
        if self.goto_after_load.is_some_and(|(d, ..)| d == id) {
            self.goto_after_load = None;
        }
    }
}

pub(super) struct Debounced<K> {
    delay: Duration,
    slots: HashMap<K, Slot>,
}

#[derive(Default)]
struct Slot {
    deadline: Option<Instant>,
    job: Option<usize>,
}

impl<K: Copy + Eq + Hash> Debounced<K> {
    pub(super) fn new(delay: Duration) -> Self {
        Self {
            delay,
            slots: HashMap::new(),
        }
    }

    pub(super) fn arm(&mut self, key: K, now: Instant) {
        self.slots.entry(key).or_default().deadline = Some(now + self.delay);
    }

    pub(super) fn is_armed(&self, key: K) -> bool {
        self.deadline(key).is_some()
    }

    pub(super) fn deadline(&self, key: K) -> Option<Instant> {
        self.slots.get(&key).and_then(|slot| slot.deadline)
    }

    pub(super) fn is_due(&self, key: K, now: Instant) -> bool {
        self.deadline(key).is_some_and(|deadline| now >= deadline)
    }

    pub(super) fn due(&self, now: Instant) -> Vec<K> {
        self.slots
            .iter()
            .filter(|(_, slot)| slot.deadline.is_some_and(|deadline| now >= deadline))
            .map(|(key, _)| *key)
            .collect()
    }

    pub(super) fn next_deadline(&self) -> Option<Instant> {
        self.slots.values().filter_map(|slot| slot.deadline).min()
    }

    pub(super) fn start(&mut self, key: K, job: usize) {
        let slot = self.slots.entry(key).or_default();
        slot.deadline = None;
        slot.job = Some(job);
    }

    pub(super) fn finish(&mut self, key: K, job: usize) {
        let Some(slot) = self.slots.get_mut(&key) else {
            return;
        };
        if slot.job == Some(job) {
            slot.job = None;
        }
        if slot.deadline.is_none() && slot.job.is_none() {
            self.slots.remove(&key);
        }
    }

    pub(super) fn cancel(&mut self, key: K) -> Option<usize> {
        self.slots.remove(&key).and_then(|slot| slot.job)
    }
}

#[cfg(test)]
#[path = "scheduler_tests.rs"]
mod scheduler_tests;
