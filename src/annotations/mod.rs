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

pub type AnnotationId = u64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationOwner {
    System,
    Lsp,
    Plugin(String),
    User,
    Remote(String),
}

impl AnnotationOwner {
    pub fn rank(&self) -> u8 {
        match self {
            AnnotationOwner::System => 0,
            AnnotationOwner::Lsp => 1,
            AnnotationOwner::Plugin(_) => 2,
            AnnotationOwner::User => 3,
            AnnotationOwner::Remote(_) => 4,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            AnnotationOwner::System => "system",
            AnnotationOwner::Lsp => "lsp",
            AnnotationOwner::Plugin(_) => "plugin",
            AnnotationOwner::User => "user",
            AnnotationOwner::Remote(_) => "remote",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Anchor {
    Point(Marker),
    Range(Marker, Marker),
    Line(usize),
}

impl Anchor {
    pub fn point(offset: usize) -> Self {
        Anchor::Point(Marker::left(offset))
    }

    pub fn range(start: usize, end: usize) -> Self {
        Anchor::Range(Marker::left(start), Marker::right(end))
    }
}

pub type LspDiagnosticSpec<'a> = (usize, Option<std::ops::Range<usize>>, i64, &'a str);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stickiness {
    Delete,
    Persist,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: AnnotationId,
    pub anchor: Anchor,
    pub kind: Kind,
    pub owner: AnnotationOwner,
    pub payload: Value,
    pub presentation: Option<Presentation>,
    pub actions: Vec<Action>,
    pub stickiness: Stickiness,
    pub visible: bool,
    pub read_only: bool,
}

impl Annotation {
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

    pub fn is_interactive(&self) -> bool {
        !self.actions.is_empty()
    }

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

    pub fn action_for_verb(&self, verb: &str) -> Option<&Action> {
        self.actions.iter().find(|a| a.verb == verb)
    }

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

pub struct AnnotationStore {
    annotations: Vec<Annotation>,
    next_id: AnnotationId,
    index: std::cell::RefCell<Option<crate::syntax::interval_tree::IntervalTree<AnnotationId>>>,
    by_id: std::cell::RefCell<std::collections::HashMap<AnnotationId, usize>>,
    interactive_starts: std::cell::RefCell<Vec<(usize, AnnotationId)>>,
    line_index: std::cell::RefCell<std::collections::BTreeMap<usize, Vec<AnnotationId>>>,
    index_dirty: std::cell::Cell<bool>,
    aux_dirty: std::cell::Cell<bool>,
    revision: u64,
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
            line_index: std::cell::RefCell::new(std::collections::BTreeMap::new()),
            index_dirty: std::cell::Cell::new(true),
            aux_dirty: std::cell::Cell::new(true),
            revision: 0,
        }
    }

    fn invalidate_index(&mut self) {
        self.index_dirty.set(true);
        self.aux_dirty.set(true);
        self.revision += 1;
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    fn ensure_aux(&self) {
        if !self.aux_dirty.get() {
            return;
        }
        let mut by_id = std::collections::HashMap::with_capacity(self.annotations.len());
        let mut line_index: std::collections::BTreeMap<usize, Vec<AnnotationId>> =
            std::collections::BTreeMap::new();
        for (idx, a) in self.annotations.iter().enumerate() {
            by_id.insert(a.id, idx);
            if let Anchor::Line(l) = a.anchor {
                line_index.entry(l).or_default().push(a.id);
            }
        }
        *self.by_id.borrow_mut() = by_id;
        *self.line_index.borrow_mut() = line_index;
        self.aux_dirty.set(false);
    }

    fn ensure_index(&self) {
        if !self.index_dirty.get() {
            return;
        }
        self.ensure_aux();
        let mut items: Vec<(std::ops::Range<usize>, AnnotationId)> = Vec::new();
        let mut starts: Vec<(usize, AnnotationId)> = Vec::new();
        for a in self.annotations.iter() {
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
        *self.interactive_starts.borrow_mut() = starts;
        self.index_dirty.set(false);
    }

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

    pub fn add(&mut self, mut annotation: Annotation) -> AnnotationId {
        let id = self.next_id;
        self.next_id += 1;
        annotation.id = id;
        self.annotations.push(annotation);
        self.invalidate_index();
        id
    }

    pub fn peek_next_id(&self) -> AnnotationId {
        self.next_id
    }

    pub fn add_with_id(&mut self, id: AnnotationId, mut annotation: Annotation) -> AnnotationId {
        annotation.id = id;
        self.annotations.push(annotation);
        self.next_id = self.next_id.max(id + 1);
        self.invalidate_index();
        id
    }

    pub fn update(&mut self, id: AnnotationId, f: impl FnOnce(&mut Annotation)) -> bool {
        if let Some(a) = self.annotations.iter_mut().find(|a| a.id == id) {
            let before = (a.anchor, a.is_interactive());
            f(a);
            self.revision += 1;
            if (a.anchor, a.is_interactive()) != before {
                self.index_dirty.set(true);
                self.aux_dirty.set(true);
            }
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, id: AnnotationId) -> bool {
        let before = self.annotations.len();
        self.annotations.retain(|a| a.id != id);
        self.invalidate_index();
        self.annotations.len() != before
    }

    pub fn get(&self, id: AnnotationId) -> Option<&Annotation> {
        self.annotations.iter().find(|a| a.id == id)
    }

    pub fn clear(&mut self) {
        self.annotations.clear();
        self.invalidate_index();
    }

    pub fn clear_by_owner(&mut self, owner: &AnnotationOwner) {
        self.annotations.retain(|a| &a.owner != owner);
        self.invalidate_index();
    }

    pub fn clear_by_kind_prefix(&mut self, prefix: &str) {
        self.annotations.retain(|a| !a.kind.matches_prefix(prefix));
        self.invalidate_index();
    }

    pub fn iter(&self) -> impl Iterator<Item = &Annotation> {
        self.annotations.iter()
    }

    pub fn query_kind<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = &'a Annotation> {
        self.annotations
            .iter()
            .filter(move |a| a.kind.matches_prefix(prefix))
    }

    pub fn query_at(&self, offset: usize) -> impl Iterator<Item = &Annotation> {
        self.index_query(offset..offset + 1).into_iter()
    }

    pub fn query_range(&self, start: usize, end: usize) -> impl Iterator<Item = &Annotation> {
        self.index_query(start..end.max(start + 1)).into_iter()
    }

    pub fn line_adornments(
        &self,
        colors: Option<&crate::color::theme::SyntaxColors>,
        defaults: Option<&registry::KindRegistry>,
        byte_range: std::ops::Range<usize>,
        line_range: std::ops::Range<usize>,
        include_lsp: bool,
        line_of: impl Fn(usize) -> usize,
    ) -> Vec<crate::render::LineAdornment<'_>> {
        let mut found: Vec<(usize, i64, &Adornment, &Annotation)> = Vec::new();
        for a in self.viewport_candidates(byte_range, line_range) {
            if !a.visible || (!include_lsp && a.owner == AnnotationOwner::Lsp) {
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
            let rank = payload::lsp::severity(&a.payload).unwrap_or(i64::MAX);
            found.push((line, rank, adornment, a));
        }
        found.sort_by_key(|(line, rank, _, _)| (*line, *rank));
        let mut out = Vec::with_capacity(found.len());
        let mut i = 0;
        while i < found.len() {
            let (line, _, adornment, a) = found[i];
            let mut j = i + 1;
            while j < found.len() && found[j].0 == line {
                j += 1;
            }
            let text = match j - i - 1 {
                0 => std::borrow::Cow::Borrowed(adornment.text.as_str()),
                extra => std::borrow::Cow::Owned(format!("{} (+{})", adornment.text, extra)),
            };
            out.push((line, text, adornment_color(a, adornment, colors, defaults)));
            i = j;
        }
        out
    }

    pub fn inline_adornments(
        &self,
        colors: Option<&crate::color::theme::SyntaxColors>,
        defaults: Option<&registry::KindRegistry>,
        range: std::ops::Range<usize>,
    ) -> Vec<(usize, usize, String, crate::color::Color, bool)> {
        let mut out = Vec::new();
        for a in self.index_query(range) {
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

    pub fn concealed_ranges(&self, range: std::ops::Range<usize>) -> Vec<(usize, usize)> {
        self.index_query(range)
            .into_iter()
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

    pub fn leading_width_in(&self, start: usize, end: usize) -> usize {
        self.index_query(start..end)
            .into_iter()
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

    fn viewport_candidates(
        &self,
        byte_range: std::ops::Range<usize>,
        line_range: std::ops::Range<usize>,
    ) -> Vec<&Annotation> {
        let mut out = self.index_query(byte_range);
        self.ensure_aux();
        let by_id = self.by_id.borrow();
        let line_ids: Vec<AnnotationId> = self
            .line_index
            .borrow()
            .range(line_range)
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect();
        for id in line_ids {
            if let Some(&idx) = by_id.get(&id) {
                out.push(&self.annotations[idx]);
            }
        }
        out
    }

    pub fn tooltip_at<'a>(
        &'a self,
        offset: usize,
        defaults: Option<&'a registry::KindRegistry>,
        include_lsp: bool,
    ) -> Option<&'a str> {
        self.query_at(offset)
            .filter(|a| a.visible && (include_lsp || a.owner != AnnotationOwner::Lsp))
            .find_map(|a| {
                payload::tooltip(&a.payload)
                    .or_else(|| defaults.and_then(|r| r.default_description(&a.kind)))
            })
    }

    pub fn tooltip_at_line<'a>(
        &'a self,
        line: usize,
        defaults: Option<&'a registry::KindRegistry>,
        include_lsp: bool,
        line_of: impl Fn(usize) -> usize,
    ) -> Option<&'a str> {
        self.annotations
            .iter()
            .filter(|a| a.visible && (include_lsp || a.owner != AnnotationOwner::Lsp))
            .filter(|a| match a.anchor {
                Anchor::Line(l) => l == line,
                Anchor::Point(p) => line_of(p.offset) == line,
                Anchor::Range(s, _) => line_of(s.offset) == line,
            })
            .filter_map(|a| {
                let tip = payload::tooltip(&a.payload)
                    .or_else(|| defaults.and_then(|r| r.default_description(&a.kind)))?;
                Some((payload::lsp::severity(&a.payload).unwrap_or(i64::MAX), tip))
            })
            .min_by_key(|(rank, _)| *rank)
            .map(|(_, tip)| tip)
    }

    pub fn interactive_at(&self, offset: usize) -> Option<&Annotation> {
        self.query_at(offset).find(|a| a.is_interactive())
    }

    pub fn interactive_at_line(&self, line: usize) -> Option<&Annotation> {
        self.annotations
            .iter()
            .find(|a| a.is_interactive() && a.anchor == Anchor::Line(line))
    }

    pub fn trailing_adornment_lines(
        &self,
        include_lsp: bool,
        line_of: impl Fn(usize) -> usize,
    ) -> Vec<usize> {
        let mut lines: Vec<usize> = self
            .annotations
            .iter()
            .filter(|a| a.visible && (include_lsp || a.owner != AnnotationOwner::Lsp))
            .filter(|a| {
                a.presentation
                    .as_ref()
                    .and_then(|p| p.adornment.as_ref())
                    .is_some_and(|ad| ad.placement == presentation::Placement::Trailing)
            })
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

    pub fn next_interactive(&self, offset: usize) -> Option<&Annotation> {
        self.ensure_index();
        let starts = self.interactive_starts.borrow();
        let i = starts.partition_point(|(s, _)| *s <= offset);
        let id = starts.get(i).map(|(_, id)| *id)?;
        self.by_id.borrow().get(&id).map(|&i| &self.annotations[i])
    }

    pub fn prev_interactive(&self, offset: usize) -> Option<&Annotation> {
        self.ensure_index();
        let starts = self.interactive_starts.borrow();
        let i = starts.partition_point(|(s, _)| *s < offset);
        let id = i
            .checked_sub(1)
            .and_then(|j| starts.get(j))
            .map(|(_, id)| *id)?;
        self.by_id.borrow().get(&id).map(|&i| &self.annotations[i])
    }

    pub fn presentation_spans(
        &self,
        colors: Option<&crate::color::theme::SyntaxColors>,
        defaults: Option<&registry::KindRegistry>,
        range: std::ops::Range<usize>,
    ) -> Vec<(std::ops::Range<usize>, crate::layer::CellStyle)> {
        type Cand = (usize, usize, crate::layer::CellStyle, i32, u8, AnnotationId);
        let mut cands: Vec<Cand> = Vec::new();
        for a in self.index_query(range) {
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

    pub fn clear_lsp_diagnostics(&mut self) {
        self.annotations.retain(|a| {
            !(a.kind.matches_prefix(well_known::LSP_DIAGNOSTIC) && a.owner == AnnotationOwner::Lsp)
        });
        self.invalidate_index();
    }

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

    fn adornment_summary(message: &str) -> String {
        let line = message.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        let mut out = String::with_capacity(line.len());
        let mut pending_space = false;
        for ch in line.chars() {
            if ch.is_whitespace() || ch.is_control() {
                pending_space = !out.is_empty();
            } else {
                if pending_space {
                    out.push(' ');
                    pending_space = false;
                }
                out.push(ch);
            }
        }
        out
    }

    fn build_diagnostic(
        line: usize,
        bytes: Option<std::ops::Range<usize>>,
        severity: i64,
        message: &str,
    ) -> Annotation {
        let sev_str = match severity {
            1 => "error",
            2 => "warning",
            3 => "info",
            4 => "hint",
            _ => "error",
        };
        let message = message.trim();
        let face = FaceRef::new(format!("diag.{}", sev_str));
        let mut payload = Value::map();
        payload.set("severity", Value::Int(severity));
        payload.set("message", Value::Str(message.to_string()));
        payload.set("tooltip", Value::Str(format!("[{}] {}", sev_str, message)));
        let underline = presentation::StyleOverride {
            underline: true,
            ..Default::default()
        };
        let presentation = Presentation::with_style(underline).with_adornment(
            presentation::Adornment::new(
                Self::adornment_summary(message),
                presentation::Placement::Trailing,
            )
            .with_face(face),
        );
        let anchor = match bytes {
            Some(r) if r.start < r.end => Anchor::range(r.start, r.end),
            _ => Anchor::Line(line),
        };
        Annotation::new(
            Kind::new(well_known::LSP_DIAGNOSTIC),
            anchor,
            AnnotationOwner::Lsp,
        )
        .with_payload(payload)
        .with_presentation(presentation)
        .with_stickiness(Stickiness::Persist)
        .with_read_only(true)
    }

    pub fn create_diagnostic(&mut self, line: usize, severity: i64, message: &str) -> AnnotationId {
        self.add(Self::build_diagnostic(line, None, severity, message))
    }

    pub fn replace_lsp_diagnostics<'a>(
        &mut self,
        diags: impl IntoIterator<Item = LspDiagnosticSpec<'a>>,
    ) {
        self.annotations.retain(|a| {
            !(a.kind.matches_prefix(well_known::LSP_DIAGNOSTIC) && a.owner == AnnotationOwner::Lsp)
        });
        for (line, bytes, severity, message) in diags {
            let mut annotation = Self::build_diagnostic(line, bytes, severity, message);
            annotation.id = self.next_id;
            self.next_id += 1;
            self.annotations.push(annotation);
        }
        self.invalidate_index();
    }

    pub fn lsp_diagnostics(&self) -> impl Iterator<Item = &Annotation> {
        self.annotations.iter().filter(|a| {
            a.kind.matches_prefix(well_known::LSP_DIAGNOSTIC) && a.owner == AnnotationOwner::Lsp
        })
    }

    const PLUGIN_HIGHLIGHT_PRIORITY: i32 = -1;

    pub fn add_plugin_highlight(
        &mut self,
        slot: u32,
        start: usize,
        end: usize,
        bg: crate::color::Color,
    ) -> AnnotationId {
        let style = StyleOverride {
            fg: Some(crate::color::contrasting_color(bg)),
            bg: Some(bg),
            ..Default::default()
        };
        self.add(
            Annotation::new(
                Kind::new(well_known::PLUGIN_HIGHLIGHT),
                Anchor::range(start, end),
                AnnotationOwner::Plugin(slot.to_string()),
            )
            .with_presentation(
                Presentation::with_style(style).with_priority(Self::PLUGIN_HIGHLIGHT_PRIORITY),
            ),
        )
    }

    pub fn clear_plugin_highlights(&mut self, slot: Option<u32>) {
        match slot {
            Some(slot) => self.clear_by_owner(&AnnotationOwner::Plugin(slot.to_string())),
            None => self.clear_by_kind_prefix(well_known::PLUGIN_HIGHLIGHT),
        }
    }

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

    pub fn create_buffer_entry(&mut self, line: usize, doc_id: u64) -> AnnotationId {
        let mut payload = Value::map();
        payload.set("doc_id", Value::Int(doc_id as i64));
        self.add(
            Annotation::new(
                Kind::new(well_known::BUFFER_ENTRY),
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

    pub fn directory_entry_info_at_line(&self, line: usize) -> Option<(String, bool)> {
        let a = self
            .annotations
            .iter()
            .find(|a| a.kind.as_str() == well_known::FS_ENTRY && a.anchor == Anchor::Line(line))?;
        let name = payload::fs::name(&a.payload)?;
        let is_dir = payload::fs::is_dir(&a.payload).unwrap_or(false);
        Some((name.to_string(), is_dir))
    }

    pub fn directory_entry_id_at_line(&self, line: usize) -> Option<u16> {
        self.annotations
            .iter()
            .find(|a| a.kind.as_str() == well_known::FS_ENTRY && a.anchor == Anchor::Line(line))
            .and_then(|a| payload::fs::entry_id(&a.payload))
    }

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

    pub fn create_git_status_head(&mut self, line: usize) -> AnnotationId {
        self.add(
            Annotation::new(
                Kind::new(well_known::GIT_STATUS_HEAD),
                Anchor::Line(line),
                AnnotationOwner::System,
            )
            .with_stickiness(Stickiness::Delete)
            .with_visible(false)
            .with_read_only(true)
            .with_actions(vec![Action::activate()]),
        )
    }

    pub fn is_git_status_head_at_line(&self, line: usize) -> bool {
        self.annotations.iter().any(|a| {
            a.kind.as_str() == well_known::GIT_STATUS_HEAD && a.anchor == Anchor::Line(line)
        })
    }

    pub fn create_git_status_entry(
        &mut self,
        line: usize,
        path: &str,
        section: &str,
        orig_path: Option<&str>,
    ) -> AnnotationId {
        let mut payload = Value::map();
        payload.set("path", Value::Str(path.to_string()));
        payload.set("section", Value::Str(section.to_string()));
        if let Some(orig) = orig_path {
            payload.set("orig_path", Value::Str(orig.to_string()));
        }
        self.add(
            Annotation::new(
                Kind::new(well_known::GIT_STATUS_ENTRY),
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

    pub fn git_status_entry_at_line(
        &self,
        line: usize,
    ) -> Option<(String, String, Option<String>)> {
        let a = self.annotations.iter().find(|a| {
            a.kind.as_str() == well_known::GIT_STATUS_ENTRY && a.anchor == Anchor::Line(line)
        })?;
        let path = payload::git::path(&a.payload)?.to_string();
        let section = payload::git::section(&a.payload)?.to_string();
        let orig_path = payload::git::orig_path(&a.payload).map(|s| s.to_string());
        Some((path, section, orig_path))
    }

    pub fn create_git_hunk(
        &mut self,
        line: usize,
        path: &str,
        staged_side: bool,
        hunk_index: usize,
    ) -> AnnotationId {
        let mut payload = Value::map();
        payload.set("path", Value::Str(path.to_string()));
        payload.set("staged_side", Value::Bool(staged_side));
        payload.set("hunk_index", Value::Int(hunk_index as i64));
        self.add(
            Annotation::new(
                Kind::new(well_known::GIT_HUNK),
                Anchor::Line(line),
                AnnotationOwner::System,
            )
            .with_payload(payload)
            .with_stickiness(Stickiness::Delete)
            .with_visible(false)
            .with_read_only(true),
        )
    }

    pub fn git_hunk_at_line(&self, line: usize) -> Option<(String, bool, usize)> {
        let a = self
            .annotations
            .iter()
            .find(|a| a.kind.as_str() == well_known::GIT_HUNK && a.anchor == Anchor::Line(line))?;
        let path = payload::git::path(&a.payload)?.to_string();
        let staged_side = payload::git::staged_side(&a.payload)?;
        let hunk_index = payload::git::hunk_index(&a.payload)?;
        Some((path, staged_side, hunk_index))
    }

    pub fn create_git_hunk_line(
        &mut self,
        line: usize,
        path: &str,
        staged_side: bool,
        hunk_index: usize,
        line_index: usize,
    ) -> AnnotationId {
        let mut payload = Value::map();
        payload.set("path", Value::Str(path.to_string()));
        payload.set("staged_side", Value::Bool(staged_side));
        payload.set("hunk_index", Value::Int(hunk_index as i64));
        payload.set("line_index", Value::Int(line_index as i64));
        self.add(
            Annotation::new(
                Kind::new(well_known::GIT_HUNK_LINE),
                Anchor::Line(line),
                AnnotationOwner::System,
            )
            .with_payload(payload)
            .with_visible(false)
            .with_read_only(true)
            .with_actions(vec![Action::activate()]),
        )
    }

    pub fn git_hunk_line_at_line(&self, line: usize) -> Option<(String, bool, usize, usize)> {
        let a = self.annotations.iter().find(|a| {
            a.kind.as_str() == well_known::GIT_HUNK_LINE && a.anchor == Anchor::Line(line)
        })?;
        let path = payload::git::path(&a.payload)?.to_string();
        let staged_side = payload::git::staged_side(&a.payload)?;
        let hunk_index = payload::git::hunk_index(&a.payload)?;
        let line_index = payload::git::line_index(&a.payload)?;
        Some((path, staged_side, hunk_index, line_index))
    }

    pub fn git_hunks_by_line(&self) -> Vec<(usize, String, bool, usize)> {
        let mut hunks: Vec<(usize, String, bool, usize)> = self
            .annotations
            .iter()
            .filter(|a| a.kind.as_str() == well_known::GIT_HUNK)
            .filter_map(|a| {
                let Anchor::Line(line) = a.anchor else {
                    return None;
                };
                let path = payload::git::path(&a.payload)?.to_string();
                let staged_side = payload::git::staged_side(&a.payload)?;
                let hunk_index = payload::git::hunk_index(&a.payload)?;
                Some((line, path, staged_side, hunk_index))
            })
            .collect();
        hunks.sort_by_key(|&(line, ..)| line);
        hunks
    }

    pub fn create_git_blame_line(
        &mut self,
        line: usize,
        source_line: usize,
        sha: &str,
    ) -> AnnotationId {
        let mut payload = Value::map();
        payload.set("sha", Value::Str(sha.to_string()));
        payload.set("source_line", Value::Int(source_line as i64));
        self.add(
            Annotation::new(
                Kind::new(well_known::GIT_BLAME),
                Anchor::Line(line),
                AnnotationOwner::System,
            )
            .with_payload(payload)
            .with_stickiness(Stickiness::Persist)
            .with_visible(false)
            .with_read_only(true)
            .with_actions(vec![Action::activate()]),
        )
    }

    pub fn git_blame_sha_at_line(&self, line: usize) -> Option<String> {
        let a = self
            .annotations
            .iter()
            .find(|a| a.kind.as_str() == well_known::GIT_BLAME && a.anchor == Anchor::Line(line))?;
        payload::git::sha(&a.payload).map(|s| s.to_string())
    }

    pub fn git_blame_source_line_at_line(&self, line: usize) -> Option<usize> {
        self.annotations
            .iter()
            .find(|a| a.kind.as_str() == well_known::GIT_BLAME && a.anchor == Anchor::Line(line))
            .and_then(|a| a.payload.get("source_line"))
            .and_then(|value| match value {
                Value::Int(line) => usize::try_from(*line).ok(),
                _ => None,
            })
    }

    pub fn git_blame_line_for_source_line(&self, source_line: usize) -> Option<usize> {
        self.annotations.iter().find_map(|annotation| {
            if annotation.kind.as_str() != well_known::GIT_BLAME {
                return None;
            }
            let Anchor::Line(line) = annotation.anchor else {
                return None;
            };
            match annotation.payload.get("source_line") {
                Some(Value::Int(value)) if usize::try_from(*value).ok() == Some(source_line) => {
                    Some(line)
                }
                _ => None,
            }
        })
    }

    pub fn create_git_log_commit(&mut self, line: usize, sha: &str) -> AnnotationId {
        let mut payload = Value::map();
        payload.set("sha", Value::Str(sha.to_string()));
        self.add(
            Annotation::new(
                Kind::new(well_known::GIT_LOG_COMMIT),
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

    pub fn git_log_commit_sha_at_line(&self, line: usize) -> Option<String> {
        let a = self.annotations.iter().find(|a| {
            a.kind.as_str() == well_known::GIT_LOG_COMMIT && a.anchor == Anchor::Line(line)
        })?;
        payload::git::sha(&a.payload).map(|s| s.to_string())
    }

    pub fn create_git_rebase_step(&mut self, line: usize, sha: &str) -> AnnotationId {
        let mut payload = Value::map();
        payload.set("sha", Value::Str(sha.to_string()));
        self.add(
            Annotation::new(
                Kind::new(well_known::GIT_REBASE_STEP),
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

    pub fn git_rebase_step_at_line(&self, line: usize) -> Option<String> {
        let a = self.annotations.iter().find(|a| {
            a.kind.as_str() == well_known::GIT_REBASE_STEP && a.anchor == Anchor::Line(line)
        })?;
        payload::git::sha(&a.payload).map(|s| s.to_string())
    }

    pub fn git_rebase_steps_by_line(&self) -> Vec<(usize, String)> {
        let mut steps: Vec<(usize, String)> = self
            .annotations
            .iter()
            .filter(|a| a.kind.as_str() == well_known::GIT_REBASE_STEP)
            .filter_map(|a| {
                let Anchor::Line(line) = a.anchor else {
                    return None;
                };
                payload::git::sha(&a.payload).map(|s| (line, s.to_string()))
            })
            .collect();
        steps.sort_by_key(|&(line, _)| line);
        steps
    }

    pub fn replace_git_gutter_signs(
        &mut self,
        signs: &[(usize, crate::git::diff::GutterSignKind)],
    ) {
        self.annotations
            .retain(|a| a.kind.as_str() != well_known::GIT_GUTTER);
        self.invalidate_index();
        for &(line, kind) in signs {
            let kind_str = match kind {
                crate::git::diff::GutterSignKind::Add => "add",
                crate::git::diff::GutterSignKind::Change => "change",
                crate::git::diff::GutterSignKind::Delete => "delete",
            };
            let mut payload = Value::map();
            payload.set("gutter_kind", Value::Str(kind_str.to_string()));
            self.add(
                Annotation::new(
                    Kind::new(well_known::GIT_GUTTER),
                    Anchor::Line(line),
                    AnnotationOwner::System,
                )
                .with_payload(payload)
                .with_stickiness(Stickiness::Persist)
                .with_visible(false)
                .with_read_only(true),
            );
        }
    }

    pub fn git_gutter_signs(&self) -> Vec<(usize, crate::git::diff::GutterSignKind)> {
        let mut signs: Vec<(usize, crate::git::diff::GutterSignKind)> = self
            .annotations
            .iter()
            .filter(|a| a.kind.as_str() == well_known::GIT_GUTTER)
            .filter_map(|a| {
                let Anchor::Line(line) = a.anchor else {
                    return None;
                };
                let kind = match payload::git::gutter_kind(&a.payload)? {
                    "add" => crate::git::diff::GutterSignKind::Add,
                    "change" => crate::git::diff::GutterSignKind::Change,
                    "delete" => crate::git::diff::GutterSignKind::Delete,
                    _ => return None,
                };
                Some((line, kind))
            })
            .collect();
        signs.sort_by_key(|&(line, _)| line);
        signs
    }

    pub fn is_empty(&self) -> bool {
        self.annotations.is_empty()
    }

    pub fn snapshot(&self) -> Vec<Annotation> {
        self.annotations
            .iter()
            .filter(|a| a.owner != AnnotationOwner::Lsp)
            .cloned()
            .collect()
    }

    pub fn restore(&mut self, mut snapshot: Vec<Annotation>) {
        snapshot.retain(|a| a.owner != AnnotationOwner::Lsp);
        snapshot.extend(
            self.annotations
                .drain(..)
                .filter(|a| a.owner == AnnotationOwner::Lsp),
        );
        self.annotations = snapshot;
        self.invalidate_index();
    }

    pub fn on_lines_deleted(&mut self, first_line: usize, count: usize, merge_line: usize) {
        if count == 0 {
            return;
        }
        let last_exclusive = first_line + count;
        self.ensure_aux();

        let affected: Vec<(usize, AnnotationId)> = self
            .line_index
            .borrow()
            .range(first_line..)
            .flat_map(|(&l, ids)| ids.iter().map(move |&id| (l, id)))
            .collect();
        if affected.is_empty() {
            return;
        }
        self.revision += 1;

        let mut to_remove: std::collections::HashSet<AnnotationId> =
            std::collections::HashSet::new();
        let mut shifted: Vec<(usize, usize, AnnotationId)> = Vec::new();
        {
            let by_id = self.by_id.borrow();
            for (l, id) in &affected {
                let Some(&idx) = by_id.get(id) else { continue };
                let new_line = if *l < last_exclusive {
                    if self.annotations[idx].stickiness == Stickiness::Delete {
                        to_remove.insert(*id);
                        continue;
                    }
                    merge_line
                } else {
                    l - count
                };
                if new_line != *l {
                    self.annotations[idx].anchor = Anchor::Line(new_line);
                    shifted.push((*l, new_line, *id));
                }
            }
        }

        if !to_remove.is_empty() {
            self.annotations.retain(|a| !to_remove.contains(&a.id));
            self.aux_dirty.set(true);
        }

        let mut line_index = self.line_index.borrow_mut();
        for (old_line, new_line, id) in shifted {
            if let Some(ids) = line_index.get_mut(&old_line) {
                ids.retain(|&i| i != id);
            }
            if line_index.get(&old_line).is_some_and(|v| v.is_empty()) {
                line_index.remove(&old_line);
            }
            line_index.entry(new_line).or_default().push(id);
        }
    }

    pub fn on_line_inserted(&mut self, at_line: usize) {
        self.ensure_aux();

        let affected: Vec<(usize, AnnotationId)> = self
            .line_index
            .borrow()
            .range(at_line..)
            .flat_map(|(&l, ids)| ids.iter().map(move |&id| (l, id)))
            .collect();
        if affected.is_empty() {
            return;
        }
        self.revision += 1;

        {
            let by_id = self.by_id.borrow();
            for (l, id) in &affected {
                if let Some(&idx) = by_id.get(id) {
                    self.annotations[idx].anchor = Anchor::Line(l + 1);
                }
            }
        }

        let mut line_index = self.line_index.borrow_mut();
        for (old_line, id) in affected {
            if let Some(ids) = line_index.get_mut(&old_line) {
                ids.retain(|&i| i != id);
            }
            if line_index.get(&old_line).is_some_and(|v| v.is_empty()) {
                line_index.remove(&old_line);
            }
            line_index.entry(old_line + 1).or_default().push(id);
        }
    }

    pub fn undo_line_inserted(&mut self, at_line: usize) {
        self.ensure_aux();

        let affected: Vec<(usize, AnnotationId)> = self
            .line_index
            .borrow()
            .range(at_line + 1..)
            .flat_map(|(&l, ids)| ids.iter().map(move |&id| (l, id)))
            .collect();
        if affected.is_empty() {
            return;
        }
        self.revision += 1;

        {
            let by_id = self.by_id.borrow();
            for (l, id) in &affected {
                if let Some(&idx) = by_id.get(id) {
                    self.annotations[idx].anchor = Anchor::Line(l - 1);
                }
            }
        }

        let mut line_index = self.line_index.borrow_mut();
        for (old_line, id) in affected {
            if let Some(ids) = line_index.get_mut(&old_line) {
                ids.retain(|&i| i != id);
            }
            if line_index.get(&old_line).is_some_and(|v| v.is_empty()) {
                line_index.remove(&old_line);
            }
            line_index.entry(old_line - 1).or_default().push(id);
        }
    }

    pub fn on_edit(&mut self, start: usize, old_end: usize, new_end: usize) {
        if start == old_end && start == new_end {
            return;
        }
        self.revision += 1;
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
                    let fully_deleted =
                        deletes && start <= s.offset && e.offset <= old_end && s.offset < e.offset;
                    s.on_edit(start, old_end, new_end);
                    e.on_edit(start, old_end, new_end);
                    if fully_deleted {
                        match a.stickiness {
                            Stickiness::Delete => to_remove.push(a.id),
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
            self.invalidate_index();
        } else {
            self.resync_index();
        }
    }

    fn resync_index(&self) {
        if self.index_dirty.get() {
            return;
        }
        let by_id = self.by_id.borrow();
        if let Some(tree) = self.index.borrow_mut().as_mut() {
            tree.resync(|id| {
                by_id
                    .get(id)
                    .and_then(|&i| match self.annotations[i].anchor {
                        Anchor::Point(p) => Some(p.offset..p.offset + 1),
                        Anchor::Range(s, e) if s.offset < e.offset => Some(s.offset..e.offset),
                        _ => None,
                    })
            });
        }
        let mut starts = self.interactive_starts.borrow_mut();
        for (start, id) in starts.iter_mut() {
            if let Some(&i) = by_id.get(id) {
                *start = match self.annotations[i].anchor {
                    Anchor::Point(p) => p.offset,
                    Anchor::Range(s, _) => s.offset,
                    Anchor::Line(_) => *start,
                };
            }
        }
        starts.sort_unstable();
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
