//! Structured sidecar metadata alongside buffer content (design.md).
//! Generic Value payload, namespaced kinds, edit-tracked markers; all serializable.

pub mod action;
pub mod kind;
pub mod marker;
pub mod payload;
pub mod presentation;
pub mod registry;
pub mod value;

pub use action::{Action, ActivationEvent, KeyHint};
pub use kind::{well_known, Kind};
pub use marker::{Gravity, Marker};
pub use presentation::{Adornment, FaceRef, Placement, Presentation, StyleOverride};
pub use value::Value;

use serde::{Deserialize, Serialize};

/// Resolve an adornment's foreground color, mirroring the range path's precedence:
/// inline adornment style, annotation style, kind-default style, then face/kind face.
fn adornment_color(
    a: &Annotation,
    adornment: &Adornment,
    colors: Option<&crate::color::theme::SyntaxColors>,
    defaults: Option<&registry::KindRegistry>,
) -> crate::color::Color {
    let own = a.presentation.as_ref();
    let kd = defaults.and_then(|r| r.default_presentation(&a.kind));
    let style_fg = adornment
        .style
        .as_ref()
        .and_then(|s| s.fg)
        .or_else(|| own.and_then(|p| p.style.as_ref()).and_then(|s| s.fg))
        .or_else(|| kd.and_then(|p| p.style.as_ref()).and_then(|s| s.fg));
    let face_fg = adornment
        .face
        .as_ref()
        .and_then(|f| presentation::resolve_face(f, colors))
        .or_else(|| {
            kd.and_then(|p| p.face.as_ref())
                .and_then(|f| presentation::resolve_face(f, colors))
        });
    style_fg
        .or(face_fg)
        .unwrap_or(crate::color::Color::DarkGrey)
}

/// Stable, unique identifier for an annotation. Does not change across edits.
pub type AnnotationId = u64;

/// Identifies the subsystem or producer that created an annotation, governing
/// lifecycle and authority (who may mutate/clear it). Orthogonal to [`Kind`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationOwner {
    /// Created by the editor core.
    System,
    /// Created by an LSP server.
    Lsp,
    /// Created by a named local plugin.
    Plugin(String),
    /// Created by the user.
    User,
    /// Reserved: a process across the IPC boundary, unused in-process today.
    Remote(String),
}

impl AnnotationOwner {
    /// Tiebreak rank for overlap precedence (lower wins).
    pub fn rank(&self) -> u8 {
        match self {
            AnnotationOwner::System => 0,
            AnnotationOwner::Lsp => 1,
            AnnotationOwner::Plugin(_) => 2,
            AnnotationOwner::User => 3,
            AnnotationOwner::Remote(_) => 4,
        }
    }

    /// Stable string tag for the owner (provenance), e.g. for the Lua snapshot.
    pub fn as_str(&self) -> &str {
        match self {
            AnnotationOwner::System => "system",
            AnnotationOwner::Lsp => "lsp",
            AnnotationOwner::Plugin(_) => "plugin",
            AnnotationOwner::User => "user",
            AnnotationOwner::Remote(_) => "remote",
        }
    }
}

/// Where an annotation lives in the buffer (design.md sec 7.3).
/// Point/Range are byte-offset markers; Line is a zero-based line number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Anchor {
    /// A single byte offset.
    Point(Marker),
    /// A byte range [start, end) with independent endpoint gravity.
    Range(Marker, Marker),
    /// An entire line identified by its zero-based line number.
    Line(usize),
}

impl Anchor {
    /// Build a point anchor with left gravity at `offset`.
    pub fn point(offset: usize) -> Self {
        Anchor::Point(Marker::left(offset))
    }

    /// Build a range over [start, end) with left start + right end gravity,
    /// so text typed inside extends it.
    pub fn range(start: usize, end: usize) -> Self {
        Anchor::Range(Marker::left(start), Marker::right(end))
    }
}

/// What happens to an annotation when its anchored span is deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stickiness {
    /// Annotation is removed when its anchor span is deleted.
    Delete,
    /// Annotation survives at the nearest remaining position.
    Persist,
}

/// A single annotation record (design.md sec 6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: AnnotationId,
    pub anchor: Anchor,
    /// Namespaced kind string.
    pub kind: Kind,
    /// Provenance and authority.
    pub owner: AnnotationOwner,
    /// Generic payload.
    pub payload: Value,
    /// Optional styling/adornment composed over base color/syntax.
    pub presentation: Option<Presentation>,
    /// Zero or more interaction descriptors.
    pub actions: Vec<Action>,
    pub stickiness: Stickiness,
    pub visible: bool,
    pub read_only: bool,
}

impl Annotation {
    /// New annotation with default fields and unassigned id (0); the id is
    /// assigned by `AnnotationStore::add`.
    pub fn new(kind: Kind, anchor: Anchor, owner: AnnotationOwner) -> Self {
        Annotation {
            id: 0,
            anchor,
            kind,
            owner,
            payload: Value::Null,
            presentation: None,
            actions: Vec::new(),
            stickiness: Stickiness::Delete,
            visible: true,
            read_only: false,
        }
    }

    pub fn with_payload(mut self, payload: Value) -> Self {
        self.payload = payload;
        self
    }

    pub fn with_stickiness(mut self, stickiness: Stickiness) -> Self {
        self.stickiness = stickiness;
        self
    }

    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn with_presentation(mut self, presentation: Presentation) -> Self {
        self.presentation = Some(presentation);
        self
    }

    pub fn with_actions(mut self, actions: Vec<Action>) -> Self {
        self.actions = actions;
        self
    }

    /// Whether this annotation has any actions (i.e. it is interactive).
    pub fn is_interactive(&self) -> bool {
        !self.actions.is_empty()
    }

    /// The action bound to the generic activate key: the one marked `default`,
    /// else the sole action if there is exactly one.
    pub fn default_action(&self) -> Option<&Action> {
        self.actions
            .iter()
            .find(|a| a.default)
            .or(if self.actions.len() == 1 {
                self.actions.first()
            } else {
                None
            })
    }

    /// The first action matching `verb`, if any.
    pub fn action_for_verb(&self, verb: &str) -> Option<&Action> {
        self.actions.iter().find(|a| a.verb == verb)
    }

    /// Keyboard-affordance hint for this annotation's actions, e.g.
    /// `"Enter: run \u{b7} t: toggle"`; `activate_key` labels the default action.
    pub fn affordance_line(&self, activate_key: &str) -> Option<String> {
        if self.actions.is_empty() {
            return None;
        }
        let parts: Vec<String> = self
            .actions
            .iter()
            .map(|a| {
                let key = match (&a.key_hint, a.default) {
                    (Some(h), _) => h.as_str(),
                    (None, true) => activate_key,
                    (None, false) => a.verb.as_str(),
                };
                format!("{}: {}", key, a.verb)
            })
            .collect();
        Some(parts.join(" \u{b7} "))
    }
}

/// Per-document annotation store; never a global singleton.
/// Position tracking is driven synchronously by the document edit pipeline.
pub struct AnnotationStore {
    annotations: Vec<Annotation>,
    next_id: AnnotationId,
    /// Lazily-rebuilt interval index over Point/Range anchors (design.md sec 10).
    index: std::cell::RefCell<Option<crate::syntax::interval_tree::IntervalTree<AnnotationId>>>,
    /// Secondary id -> vec-index map, rebuilt with the index for O(1) id lookup.
    by_id: std::cell::RefCell<std::collections::HashMap<AnnotationId, usize>>,
    /// (start_offset, id) of interactive Point/Range anchors, sorted by offset,
    /// rebuilt with the index. Backs O(log n) next/prev-interactive navigation.
    interactive_starts: std::cell::RefCell<Vec<(usize, AnnotationId)>>,
    index_dirty: std::cell::Cell<bool>,
}

impl Default for AnnotationStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AnnotationStore {
    pub fn new() -> Self {
        Self {
            annotations: Vec::new(),
            next_id: 1,
            index: std::cell::RefCell::new(None),
            by_id: std::cell::RefCell::new(std::collections::HashMap::new()),
            interactive_starts: std::cell::RefCell::new(Vec::new()),
            index_dirty: std::cell::Cell::new(true),
        }
    }

    /// Mark the spatial index stale; rebuilt on the next offset query.
    fn invalidate_index(&self) {
        self.index_dirty.set(true);
    }

    /// Rebuild the interval index + id map from anchors if stale.
    fn ensure_index(&self) {
        if !self.index_dirty.get() {
            return;
        }
        let mut items: Vec<(std::ops::Range<usize>, AnnotationId)> = Vec::new();
        let mut by_id = std::collections::HashMap::with_capacity(self.annotations.len());
        let mut starts: Vec<(usize, AnnotationId)> = Vec::new();
        for (idx, a) in self.annotations.iter().enumerate() {
            by_id.insert(a.id, idx);
            let start = match a.anchor {
                Anchor::Point(p) => {
                    items.push((p.offset..p.offset + 1, a.id));
                    Some(p.offset)
                }
                Anchor::Range(s, e) if s.offset < e.offset => {
                    items.push((s.offset..e.offset, a.id));
                    Some(s.offset)
                }
                _ => None,
            };
            if let Some(start) = start {
                if a.is_interactive() {
                    starts.push((start, a.id));
                }
            }
        }
        starts.sort_unstable();
        *self.index.borrow_mut() = Some(crate::syntax::interval_tree::IntervalTree::new(items));
        *self.by_id.borrow_mut() = by_id;
        *self.interactive_starts.borrow_mut() = starts;
        self.index_dirty.set(false);
    }

    /// Annotations whose Point/Range anchor overlaps a byte range, in id order,
    /// resolved through the interval index + id map.
    fn index_query(&self, range: std::ops::Range<usize>) -> Vec<&Annotation> {
        self.ensure_index();
        let mut ids: Vec<AnnotationId> = self
            .index
            .borrow()
            .as_ref()
            .map(|t| t.query(range).into_iter().map(|(_, id)| id).collect())
            .unwrap_or_default();
        ids.sort_unstable();
        ids.dedup();
        let by_id = self.by_id.borrow();
        ids.into_iter()
            .filter_map(|id| by_id.get(&id).map(|&i| &self.annotations[i]))
            .collect()
    }

    // -- Generalized lifecycle (design.md sec 10) --

    /// Add an annotation, assigning it a fresh stable id (overwriting any id on
    /// the passed value). Returns the assigned id.
    pub fn add(&mut self, mut annotation: Annotation) -> AnnotationId {
        let id = self.next_id;
        self.next_id += 1;
        annotation.id = id;
        self.annotations.push(annotation);
        self.invalidate_index();
        id
    }

    /// The id the next `add` will assign. Lets a deferred producer (the Lua host)
    /// pre-claim ids so `add` can report one synchronously.
    pub fn peek_next_id(&self) -> AnnotationId {
        self.next_id
    }

    /// Add an annotation under a caller-chosen id (e.g. one pre-claimed via
    /// `peek_next_id`), keeping `next_id` ahead so later allocations never collide.
    pub fn add_with_id(&mut self, id: AnnotationId, mut annotation: Annotation) -> AnnotationId {
        annotation.id = id;
        self.annotations.push(annotation);
        self.next_id = self.next_id.max(id + 1);
        self.invalidate_index();
        id
    }

    /// Mutate the annotation with the given id in place. Returns `false` if no
    /// such annotation exists.
    pub fn update(&mut self, id: AnnotationId, f: impl FnOnce(&mut Annotation)) -> bool {
        if let Some(a) = self.annotations.iter_mut().find(|a| a.id == id) {
            f(a);
            self.invalidate_index();
            true
        } else {
            false
        }
    }

    /// Remove the annotation with the given id. Returns `true` if one was removed.
    pub fn remove(&mut self, id: AnnotationId) -> bool {
        let before = self.annotations.len();
        self.annotations.retain(|a| a.id != id);
        self.invalidate_index();
        self.annotations.len() != before
    }

    /// Look up an annotation by id.
    pub fn get(&self, id: AnnotationId) -> Option<&Annotation> {
        self.annotations.iter().find(|a| a.id == id)
    }

    /// Remove all annotations from the store.
    pub fn clear(&mut self) {
        self.annotations.clear();
        self.invalidate_index();
    }

    /// Remove all annotations created by a given owner.
    pub fn clear_by_owner(&mut self, owner: &AnnotationOwner) {
        self.annotations.retain(|a| &a.owner != owner);
        self.invalidate_index();
    }

    /// Remove all annotations whose kind matches a prefix (e.g. `"lsp."`).
    pub fn clear_by_kind_prefix(&mut self, prefix: &str) {
        self.annotations.retain(|a| !a.kind.matches_prefix(prefix));
        self.invalidate_index();
    }

    // -- Queries (design.md sec 10) --

    /// All annotations, in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &Annotation> {
        self.annotations.iter()
    }

    /// Annotations whose kind matches a prefix.
    pub fn query_kind<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = &'a Annotation> {
        self.annotations
            .iter()
            .filter(move |a| a.kind.matches_prefix(prefix))
    }

    /// Annotations covering a byte offset (`Point` at the offset, or `Range`
    /// containing it). `Line` anchors are not offset-addressable and are excluded.
    pub fn query_at(&self, offset: usize) -> impl Iterator<Item = &Annotation> {
        self.index_query(offset..offset + 1).into_iter()
    }

    /// Annotations overlapping a byte range [start, end).
    pub fn query_range(&self, start: usize, end: usize) -> impl Iterator<Item = &Annotation> {
        self.index_query(start..end.max(start + 1)).into_iter()
    }

    /// Trailing end-of-line adornments as (line, text, color); offset anchors map
    /// to a line via `line_of`. Drives diagnostic/blame virtual text (design.md sec 8).
    pub fn line_adornments(
        &self,
        colors: Option<&crate::color::theme::SyntaxColors>,
        defaults: Option<&registry::KindRegistry>,
        line_of: impl Fn(usize) -> usize,
    ) -> Vec<(usize, String, crate::color::Color)> {
        let mut out = Vec::new();
        for a in &self.annotations {
            if !a.visible {
                continue;
            }
            let Some(adornment) = a.presentation.as_ref().and_then(|p| p.adornment.as_ref()) else {
                continue;
            };
            if adornment.placement != presentation::Placement::Trailing {
                continue;
            }
            let line = match a.anchor {
                Anchor::Line(l) => l,
                Anchor::Point(p) => line_of(p.offset),
                Anchor::Range(s, _) => line_of(s.offset),
            };
            let color = adornment_color(a, adornment, colors, defaults);
            out.push((line, adornment.text.clone(), color));
        }
        out
    }

    /// Inline (Overlay/Leading) adornments as (start, end, text, color, is_leading);
    /// Overlay end = range end (or point width), Leading end = start (insertion).
    pub fn inline_adornments(
        &self,
        colors: Option<&crate::color::theme::SyntaxColors>,
        defaults: Option<&registry::KindRegistry>,
    ) -> Vec<(usize, usize, String, crate::color::Color, bool)> {
        let mut out = Vec::new();
        for a in &self.annotations {
            if !a.visible {
                continue;
            }
            let Some(adornment) = a.presentation.as_ref().and_then(|p| p.adornment.as_ref()) else {
                continue;
            };
            let is_leading = match adornment.placement {
                presentation::Placement::Overlay => false,
                presentation::Placement::Leading => true,
                presentation::Placement::Trailing | presentation::Placement::Conceal => continue,
            };
            let (start, end) = match a.anchor {
                Anchor::Point(p) => (p.offset, p.offset),
                // Leading inserts at the start; Overlay conceals the whole range.
                Anchor::Range(s, e) if is_leading => (s.offset, s.offset),
                Anchor::Range(s, e) => (s.offset, e.offset),
                Anchor::Line(_) => continue,
            };
            let color = adornment_color(a, adornment, colors, defaults);
            out.push((start, end, adornment.text.clone(), color, is_leading));
        }
        out.sort_by_key(|(s, _, _, _, _)| *s);
        out
    }

    /// Byte ranges hidden by Conceal adornments (zero display width). The renderer
    /// skips these cells except on the cursor's own line (design.md sec 8).
    pub fn concealed_ranges(&self) -> Vec<(usize, usize)> {
        self.annotations
            .iter()
            .filter(|a| a.visible)
            .filter_map(|a| {
                let ad = a.presentation.as_ref()?.adornment.as_ref()?;
                if ad.placement != presentation::Placement::Conceal {
                    return None;
                }
                match a.anchor {
                    Anchor::Range(s, e) if s.offset < e.offset => Some((s.offset, e.offset)),
                    _ => None,
                }
            })
            .collect()
    }

    /// Total display width of Leading adornments anchored in `[start, end)`, used
    /// to offset the cursor column for virtual text inserted before it.
    pub fn leading_width_in(&self, start: usize, end: usize) -> usize {
        self.annotations
            .iter()
            .filter(|a| a.visible)
            .filter_map(|a| {
                let ad = a.presentation.as_ref()?.adornment.as_ref()?;
                if ad.placement != presentation::Placement::Leading {
                    return None;
                }
                let off = match a.anchor {
                    Anchor::Point(p) => p.offset,
                    Anchor::Range(s, _) => s.offset,
                    Anchor::Line(_) => return None,
                };
                (off >= start && off < end).then(|| ad.text.chars().count())
            })
            .sum()
    }

    /// First visible annotation tooltip covering a byte offset, falling back to
    /// the kind's default description when the payload has none.
    pub fn tooltip_at<'a>(
        &'a self,
        offset: usize,
        defaults: Option<&'a registry::KindRegistry>,
    ) -> Option<&'a str> {
        self.query_at(offset).filter(|a| a.visible).find_map(|a| {
            payload::tooltip(&a.payload)
                .or_else(|| defaults.and_then(|r| r.default_description(&a.kind)))
        })
    }

    /// The first visible annotation tooltip on a line (line-anchored), with the
    /// same kind-default-description fallback as `tooltip_at`.
    pub fn tooltip_at_line<'a>(
        &'a self,
        line: usize,
        defaults: Option<&'a registry::KindRegistry>,
    ) -> Option<&'a str> {
        self.annotations
            .iter()
            .filter(|a| a.visible && a.anchor == Anchor::Line(line))
            .find_map(|a| {
                payload::tooltip(&a.payload)
                    .or_else(|| defaults.and_then(|r| r.default_description(&a.kind)))
            })
    }

    // -- Interactive navigation (design.md sec 9.4) --

    /// The interactive annotation whose span covers `offset`, if any.
    pub fn interactive_at(&self, offset: usize) -> Option<&Annotation> {
        self.query_at(offset).find(|a| a.is_interactive())
    }

    /// The interactive annotation anchored at `line`, if any (for line-anchored
    /// interface buffers like the file explorer).
    pub fn interactive_at_line(&self, line: usize) -> Option<&Annotation> {
        self.annotations
            .iter()
            .find(|a| a.is_interactive() && a.anchor == Anchor::Line(line))
    }

    /// Sorted, de-duplicated lines carrying an interactive annotation (offset
    /// anchors mapped via `line_of`). Drives interface-mode snapping (sec 9.4).
    pub fn interactive_lines(&self, line_of: impl Fn(usize) -> usize) -> Vec<usize> {
        let mut lines: Vec<usize> = self
            .annotations
            .iter()
            .filter(|a| a.is_interactive())
            .map(|a| match a.anchor {
                Anchor::Line(l) => l,
                Anchor::Point(p) => line_of(p.offset),
                Anchor::Range(s, _) => line_of(s.offset),
            })
            .collect();
        lines.sort_unstable();
        lines.dedup();
        lines
    }

    /// The next interactive annotation starting strictly after `offset`, found by
    /// binary search over the sorted interactive-starts index (design.md sec 10).
    pub fn next_interactive(&self, offset: usize) -> Option<&Annotation> {
        self.ensure_index();
        let starts = self.interactive_starts.borrow();
        // First entry with start > offset (entries are sorted by (start, id)).
        let i = starts.partition_point(|(s, _)| *s <= offset);
        let id = starts.get(i).map(|(_, id)| *id)?;
        self.by_id.borrow().get(&id).map(|&i| &self.annotations[i])
    }

    /// The previous interactive annotation starting strictly before `offset`,
    /// found by binary search over the sorted interactive-starts index.
    pub fn prev_interactive(&self, offset: usize) -> Option<&Annotation> {
        self.ensure_index();
        let starts = self.interactive_starts.borrow();
        // Last entry with start < offset.
        let i = starts.partition_point(|(s, _)| *s < offset);
        let id = i
            .checked_sub(1)
            .and_then(|j| starts.get(j))
            .map(|(_, id)| *id)?;
        self.by_id.borrow().get(&id).map(|&i| &self.annotations[i])
    }

    // -- Presentation (design.md sec 8) --

    /// Flatten visible range/point presentations into sorted, non-overlapping
    /// style spans (fg/bg/attrs); higher precedence wins each overlapping cell.
    pub fn presentation_spans(
        &self,
        colors: Option<&crate::color::theme::SyntaxColors>,
        defaults: Option<&registry::KindRegistry>,
    ) -> Vec<(std::ops::Range<usize>, crate::layer::CellStyle)> {
        // (start, end, style, priority, owner_rank, id) per styled annotation.
        type Cand = (usize, usize, crate::layer::CellStyle, i32, u8, AnnotationId);
        let mut cands: Vec<Cand> = Vec::new();
        for a in &self.annotations {
            if !a.visible {
                continue;
            }
            let (start, end) = match a.anchor {
                Anchor::Range(s, e) if s.offset < e.offset => (s.offset, e.offset),
                Anchor::Point(p) => (p.offset, p.offset + 1),
                _ => continue,
            };
            let pres = a
                .presentation
                .as_ref()
                .or_else(|| defaults.and_then(|r| r.default_presentation(&a.kind)));
            let Some(pres) = pres else {
                continue;
            };
            let face_fg = pres
                .face
                .as_ref()
                .and_then(|f| presentation::resolve_face(f, colors));
            let fg = pres.style.as_ref().and_then(|s| s.fg).or(face_fg);
            let bg = pres.style.as_ref().and_then(|s| s.bg);
            let attrs = pres.style.as_ref().map(|s| s.attrs()).unwrap_or_default();
            if fg.is_some() || bg.is_some() || !attrs.is_empty() {
                let style = crate::layer::CellStyle { fg, bg, attrs };
                cands.push((start, end, style, pres.priority, a.owner.rank(), a.id));
            }
        }
        if cands.is_empty() {
            return Vec::new();
        }

        // Elementary intervals between all boundaries; pick the best candidate
        // covering each, then merge adjacent equal-style segments.
        let mut bounds: Vec<usize> = Vec::with_capacity(cands.len() * 2);
        for c in &cands {
            bounds.push(c.0);
            bounds.push(c.1);
        }
        bounds.sort_unstable();
        bounds.dedup();

        let mut spans: Vec<(std::ops::Range<usize>, crate::layer::CellStyle)> = Vec::new();
        for w in bounds.windows(2) {
            let (seg_s, seg_e) = (w[0], w[1]);
            let best = cands
                .iter()
                .filter(|c| c.0 <= seg_s && seg_e <= c.1)
                // higher priority, then lower owner rank, then higher id.
                .max_by(|a, b| a.3.cmp(&b.3).then(b.4.cmp(&a.4)).then(a.5.cmp(&b.5)));
            if let Some(best) = best {
                if let Some(last) = spans.last_mut() {
                    if last.0.end == seg_s && last.1 == best.2 {
                        last.0.end = seg_e;
                        continue;
                    }
                }
                spans.push((seg_s..seg_e, best.2));
            }
        }
        spans
    }

    // -- Thin typed wrappers (preserve existing call-site behavior) --------

    /// Remove all LSP diagnostic annotations (refresh before new diagnostics arrive).
    pub fn clear_lsp_diagnostics(&mut self) {
        self.annotations.retain(|a| {
            !(a.kind.matches_prefix(well_known::LSP_DIAGNOSTIC) && a.owner == AnnotationOwner::Lsp)
        });
    }

    /// Create an LSP diagnostic annotation anchored at `line` with a tooltip message.
    pub fn create_lsp_diagnostic(&mut self, line: usize, tooltip: String) -> AnnotationId {
        let mut payload = Value::map();
        payload.set("tooltip", Value::Str(tooltip));
        self.add(
            Annotation::new(
                Kind::new(well_known::LSP_DIAGNOSTIC),
                Anchor::Line(line),
                AnnotationOwner::Lsp,
            )
            .with_payload(payload)
            .with_stickiness(Stickiness::Persist)
            .with_read_only(true),
        )
    }

    /// Create an LSP diagnostic: a `diag.<sev>` face plus a trailing EOL message
    /// adornment, rendered through the generic presentation path (design.md sec 8).
    pub fn create_diagnostic(&mut self, line: usize, severity: i64, message: &str) -> AnnotationId {
        let sev_str = match severity {
            1 => "error",
            2 => "warning",
            3 => "info",
            4 => "hint",
            _ => "error",
        };
        let face = FaceRef::new(format!("diag.{}", sev_str));
        let mut payload = Value::map();
        payload.set("severity", Value::Int(severity));
        payload.set("message", Value::Str(message.to_string()));
        payload.set("tooltip", Value::Str(format!("[{}] {}", sev_str, message)));
        let presentation = Presentation::with_face(face.clone()).with_adornment(
            presentation::Adornment::new(message, presentation::Placement::Trailing)
                .with_face(face),
        );
        self.add(
            Annotation::new(
                Kind::new(well_known::LSP_DIAGNOSTIC),
                Anchor::Line(line),
                AnnotationOwner::Lsp,
            )
            .with_payload(payload)
            .with_presentation(presentation)
            .with_stickiness(Stickiness::Persist)
            .with_read_only(true),
        )
    }

    /// Return all LSP diagnostic annotations.
    pub fn lsp_diagnostics(&self) -> impl Iterator<Item = &Annotation> {
        self.annotations.iter().filter(|a| {
            a.kind.matches_prefix(well_known::LSP_DIAGNOSTIC) && a.owner == AnnotationOwner::Lsp
        })
    }

    /// Create an `fs.entry` annotation anchored at `line` with a stable entry ID.
    pub fn create_directory_entry(&mut self, line: usize, entry_id: u16) -> AnnotationId {
        let mut payload = Value::map();
        payload.set("entry_id", Value::Int(entry_id as i64));
        self.add(
            Annotation::new(
                Kind::new(well_known::FS_ENTRY),
                Anchor::Line(line),
                AnnotationOwner::System,
            )
            .with_payload(payload)
            .with_stickiness(Stickiness::Delete)
            .with_visible(false)
            .with_read_only(true),
        )
    }

    /// Create an interactive `fs.entry` annotation carrying its display name and
    /// directory flag in the payload, plus an `activate` action (design.md sec 13).
    pub fn create_fs_entry(
        &mut self,
        line: usize,
        entry_id: u16,
        name: &str,
        is_dir: bool,
    ) -> AnnotationId {
        let mut payload = Value::map();
        payload.set("entry_id", Value::Int(entry_id as i64));
        payload.set("name", Value::Str(name.to_string()));
        payload.set("is_dir", Value::Bool(is_dir));
        self.add(
            Annotation::new(
                Kind::new(well_known::FS_ENTRY),
                Anchor::Line(line),
                AnnotationOwner::System,
            )
            .with_payload(payload)
            .with_stickiness(Stickiness::Delete)
            .with_visible(false)
            .with_read_only(true)
            .with_actions(vec![Action::activate()]),
        )
    }

    /// The `(name, is_dir)` for the `fs.entry` at `line`, if its payload carries
    /// them (entries restored from undo snapshots may not).
    pub fn directory_entry_info_at_line(&self, line: usize) -> Option<(String, bool)> {
        let a = self
            .annotations
            .iter()
            .find(|a| a.kind.as_str() == well_known::FS_ENTRY && a.anchor == Anchor::Line(line))?;
        let name = payload::fs::name(&a.payload)?;
        let is_dir = payload::fs::is_dir(&a.payload).unwrap_or(false);
        Some((name.to_string(), is_dir))
    }

    /// Return the directory entry ID for the `fs.entry` annotation anchored at
    /// the given line, or `None` if none exists there.
    pub fn directory_entry_id_at_line(&self, line: usize) -> Option<u16> {
        self.annotations
            .iter()
            .find(|a| a.kind.as_str() == well_known::FS_ENTRY && a.anchor == Anchor::Line(line))
            .and_then(|a| payload::fs::entry_id(&a.payload))
    }

    /// Return all `(line, entry_id)` pairs for `fs.entry` annotations, sorted by line.
    pub fn directory_entries_by_line(&self) -> Vec<(usize, u16)> {
        let mut entries: Vec<(usize, u16)> = self
            .annotations
            .iter()
            .filter(|a| a.kind.as_str() == well_known::FS_ENTRY)
            .filter_map(|a| {
                if let Anchor::Line(l) = a.anchor {
                    payload::fs::entry_id(&a.payload).map(|eid| (l, eid))
                } else {
                    None
                }
            })
            .collect();
        entries.sort_by_key(|&(l, _)| l);
        entries
    }

    /// Whether the store holds no annotations.
    pub fn is_empty(&self) -> bool {
        self.annotations.is_empty()
    }

    /// Clone the full annotation state for an undo/redo snapshot.
    pub fn snapshot(&self) -> Vec<Annotation> {
        self.annotations.clone()
    }

    /// Replace all annotations with a previously captured snapshot. `next_id` is
    /// left untouched (it only grows), so restored ids never collide with new ones.
    pub fn restore(&mut self, snapshot: Vec<Annotation>) {
        self.annotations = snapshot;
        self.invalidate_index();
    }

    // -- Line-anchor edit tracking --

    /// Update Line anchors after `count` lines are deleted from `first_line`.
    /// Delete-sticky annotations in range are removed; later lines shift down.
    pub fn on_lines_deleted(&mut self, first_line: usize, count: usize) {
        if count == 0 {
            return;
        }
        let last_exclusive = first_line + count;
        self.annotations.retain(|a| {
            if let Anchor::Line(l) = a.anchor {
                if l >= first_line && l < last_exclusive {
                    a.stickiness == Stickiness::Persist
                } else {
                    true
                }
            } else {
                true
            }
        });
        for a in &mut self.annotations {
            if let Anchor::Line(ref mut l) = a.anchor {
                if *l >= last_exclusive {
                    *l -= count;
                }
            }
        }
        self.invalidate_index();
    }

    /// Update Line anchors after a new line is inserted at `at_line`.
    pub fn on_line_inserted(&mut self, at_line: usize) {
        for a in &mut self.annotations {
            if let Anchor::Line(ref mut l) = a.anchor {
                if *l >= at_line {
                    *l += 1;
                }
            }
        }
        self.invalidate_index();
    }

    /// Maintain Point/Range markers for an edit replacing [start, old_end) with
    /// new_end-start bytes; applies range stickiness. Line anchors are untouched.
    pub fn on_edit(&mut self, start: usize, old_end: usize, new_end: usize) {
        if start == old_end && start == new_end {
            return;
        }
        let deletes = old_end > start;

        let mut to_remove: Vec<AnnotationId> = Vec::new();
        for a in &mut self.annotations {
            match a.anchor {
                Anchor::Point(ref mut m) => {
                    let inside = deletes && start < m.offset && m.offset < old_end;
                    m.on_edit(start, old_end, new_end);
                    if inside && a.stickiness == Stickiness::Delete {
                        to_remove.push(a.id);
                    }
                }
                Anchor::Range(ref mut s, ref mut e) => {
                    // Fully deleted: the deletion covers the entire span.
                    let fully_deleted =
                        deletes && start <= s.offset && e.offset <= old_end && s.offset < e.offset;
                    s.on_edit(start, old_end, new_end);
                    e.on_edit(start, old_end, new_end);
                    if fully_deleted {
                        match a.stickiness {
                            Stickiness::Delete => to_remove.push(a.id),
                            // Persist: collapse to a zero-width point at the boundary.
                            Stickiness::Persist => {
                                a.anchor = Anchor::Point(Marker::left(s.offset));
                            }
                        }
                    }
                }
                Anchor::Line(_) => {}
            }
        }
        if !to_remove.is_empty() {
            self.annotations.retain(|a| !to_remove.contains(&a.id));
        }
        self.invalidate_index();
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
