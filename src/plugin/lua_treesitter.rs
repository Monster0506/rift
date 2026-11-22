use super::{buf_or_current, lua_index, lua_int, LuaSharedState};
use crate::document::DocumentId;
use crate::plugin::PluginMutation;
use crate::syntax::loader::QueryKind;
use crate::syntax::snapshot::{QueryWindow, SnapshotText, SyntaxSnapshot};
use mlua::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use tree_sitter::{Node, Point, Query, QueryMatch};

type Shared = Rc<RefCell<LuaSharedState>>;

struct LuaTree(Rc<SyntaxSnapshot>);

pub(super) struct LuaNode {
    snap: Rc<SyntaxSnapshot>,
    raw: tree_sitter::ffi::TSNode,
}

struct LuaQuery {
    lang: String,
    query: Rc<Query>,
}

impl LuaNode {
    fn new(snap: &Rc<SyntaxSnapshot>, node: Node<'_>) -> Self {
        Self {
            snap: Rc::clone(snap),
            raw: node.into_raw(),
        }
    }

    fn node(&self) -> Node<'_> {
        unsafe { Node::from_raw(self.raw) }
    }

    fn wrap(&self, node: Option<Node<'_>>) -> Option<Self> {
        node.map(|n| Self::new(&self.snap, n))
    }
}

pub(super) fn node_extent(ud: &LuaAnyUserData) -> Option<(Option<u64>, std::ops::Range<usize>)> {
    let node = ud.borrow::<LuaNode>().ok()?;
    Some((node.snap.buf, node.node().byte_range()))
}

fn lua_point(p: Point) -> (i64, i64) {
    (lua_int(p.row) + 1, lua_int(p.column))
}

fn ts_point(row: i64, col: i64) -> Point {
    Point::new(lua_index(row - 1), lua_index(col))
}

fn child_index(i: i64) -> Option<u32> {
    u32::try_from(i.checked_sub(1)?).ok()
}

impl LuaUserData for LuaNode {
    #[allow(clippy::too_many_lines)]
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("type", |_, this, ()| Ok(this.node().kind().to_string()));
        methods.add_method("named", |_, this, ()| Ok(this.node().is_named()));
        methods.add_method("missing", |_, this, ()| Ok(this.node().is_missing()));
        methods.add_method("extra", |_, this, ()| Ok(this.node().is_extra()));
        methods.add_method("has_error", |_, this, ()| Ok(this.node().has_error()));
        methods.add_method("is_error", |_, this, ()| Ok(this.node().is_error()));
        methods.add_method("range", |_, this, ()| {
            let n = this.node();
            let (sr, sc) = lua_point(n.start_position());
            let (er, ec) = lua_point(n.end_position());
            Ok((sr, sc, er, ec))
        });
        methods.add_method("start", |_, this, ()| {
            let n = this.node();
            let (row, col) = lua_point(n.start_position());
            Ok((row, col, lua_int(n.start_byte())))
        });
        methods.add_method("end_", |_, this, ()| {
            let n = this.node();
            let (row, col) = lua_point(n.end_position());
            Ok((row, col, lua_int(n.end_byte())))
        });
        methods.add_method("byte_range", |_, this, ()| {
            let n = this.node();
            Ok((lua_int(n.start_byte()), lua_int(n.end_byte())))
        });
        methods.add_method("text", |lua, this, ()| {
            lua.create_string(this.snap.node_text(this.node()))
        });
        methods.add_method("sexpr", |_, this, ()| Ok(this.node().to_sexp()));
        methods.add_method("id", |_, this, ()| Ok(lua_int(this.node().id())));
        methods.add_method("lang", |_, this, ()| Ok(this.snap.lang_name.clone()));
        methods.add_method("buf", |_, this, ()| Ok(this.snap.buf));
        methods.add_method("tree", |_, this, ()| Ok(LuaTree(Rc::clone(&this.snap))));
        methods.add_method("parent", |_, this, ()| Ok(this.wrap(this.node().parent())));
        methods.add_method("child_count", |_, this, ()| Ok(this.node().child_count()));
        methods.add_method("named_child_count", |_, this, ()| {
            Ok(lua_int(this.node().named_child_count()))
        });
        methods.add_method("child", |_, this, i: i64| {
            Ok(child_index(i).and_then(|i| this.wrap(this.node().child(i))))
        });
        methods.add_method("named_child", |_, this, i: i64| {
            Ok(child_index(i).and_then(|i| this.wrap(this.node().named_child(i))))
        });
        methods.add_method("children", |_, this, ()| {
            let n = this.node();
            let mut cursor = n.walk();
            let out: Vec<Self> = n
                .children(&mut cursor)
                .map(|c| Self::new(&this.snap, c))
                .collect();
            Ok(out)
        });
        methods.add_method("named_children", |_, this, ()| {
            let n = this.node();
            let mut cursor = n.walk();
            let out: Vec<Self> = n
                .named_children(&mut cursor)
                .map(|c| Self::new(&this.snap, c))
                .collect();
            Ok(out)
        });
        methods.add_method("iter_children", |lua, this, ()| {
            let n = this.node();
            let mut cursor = n.walk();
            let mut items: Vec<(Self, Option<String>)> = Vec::new();
            if cursor.goto_first_child() {
                loop {
                    items.push((
                        Self::new(&this.snap, cursor.node()),
                        cursor.field_name().map(str::to_string),
                    ));
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
            let mut items = items.into_iter();
            lua.create_function_mut(move |_, ()| {
                Ok(items.next().map_or((None, None), |(n, f)| (Some(n), f)))
            })
        });
        methods.add_method("field", |_, this, name: String| {
            let n = this.node();
            let mut cursor = n.walk();
            let out: Vec<Self> = n
                .children_by_field_name(&name, &mut cursor)
                .map(|c| Self::new(&this.snap, c))
                .collect();
            Ok(out)
        });
        methods.add_method("child_by_field", |_, this, name: String| {
            Ok(this.wrap(this.node().child_by_field_name(&name)))
        });
        methods.add_method("next_sibling", |_, this, ()| {
            Ok(this.wrap(this.node().next_sibling()))
        });
        methods.add_method("prev_sibling", |_, this, ()| {
            Ok(this.wrap(this.node().prev_sibling()))
        });
        methods.add_method("next_named_sibling", |_, this, ()| {
            Ok(this.wrap(this.node().next_named_sibling()))
        });
        methods.add_method("prev_named_sibling", |_, this, ()| {
            Ok(this.wrap(this.node().prev_named_sibling()))
        });
        methods.add_method(
            "descendant_for_range",
            |_, this, (sr, sc, er, ec): (i64, i64, i64, i64)| {
                Ok(this.wrap(
                    this.node()
                        .descendant_for_point_range(ts_point(sr, sc), ts_point(er, ec)),
                ))
            },
        );
        methods.add_method(
            "named_descendant_for_range",
            |_, this, (sr, sc, er, ec): (i64, i64, i64, i64)| {
                Ok(this.wrap(
                    this.node()
                        .named_descendant_for_point_range(ts_point(sr, sc), ts_point(er, ec)),
                ))
            },
        );
        methods.add_meta_method(LuaMetaMethod::Eq, |_, this, other: LuaAnyUserData| {
            Ok(other
                .borrow::<Self>()
                .is_ok_and(|o| Rc::ptr_eq(&this.snap, &o.snap) && this.node() == o.node()))
        });
        methods.add_meta_method(LuaMetaMethod::ToString, |_, this, ()| {
            let n = this.node();
            let (sr, sc) = lua_point(n.start_position());
            let (er, ec) = lua_point(n.end_position());
            Ok(format!("<node {} {sr}:{sc}-{er}:{ec}>", n.kind()))
        });
    }
}

impl LuaUserData for LuaTree {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("root", |_, this, ()| {
            Ok(LuaNode::new(&this.0, this.0.root()))
        });
        methods.add_method("lang", |_, this, ()| Ok(this.0.lang_name.clone()));
        methods.add_method("buf", |_, this, ()| Ok(this.0.buf));
        methods.add_method("is_stale", |lua, this, ()| {
            let (Some(buf), SnapshotText::Buffer(text)) = (this.0.buf, &*this.0.text) else {
                return Ok(false);
            };
            let shared = lua
                .app_data_ref::<Shared>()
                .ok_or_else(|| LuaError::RuntimeError("plugin state unavailable".into()))?;
            let s = shared.borrow();
            Ok(s.bufs
                .get(&buf)
                .is_some_and(|snap| snap.buffer.revision != text.revision))
        });
        methods.add_meta_method(LuaMetaMethod::ToString, |_, this, ()| {
            Ok(format!("<tree {}>", this.0.lang_name))
        });
    }
}

impl LuaQuery {
    fn target(&self, node: &LuaAnyUserData) -> LuaResult<LuaNode> {
        let node = node.borrow::<LuaNode>()?;
        if node.snap.lang_name != self.lang {
            return Err(LuaError::RuntimeError(format!(
                "query for '{}' cannot run on a '{}' node",
                self.lang, node.snap.lang_name
            )));
        }
        Ok(LuaNode {
            snap: Rc::clone(&node.snap),
            raw: node.raw,
        })
    }
}

fn row_window(start: Option<i64>, end: Option<i64>) -> Option<QueryWindow> {
    match (start, end) {
        (None, None) => None,
        (s, e) => Some(QueryWindow::Rows(
            lua_index(s.unwrap_or(1) - 1)..e.map_or(usize::MAX, lua_index),
        )),
    }
}

fn metadata(lua: &Lua, query: &Query, pattern: usize) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    for prop in query.property_settings(pattern) {
        match &prop.value {
            Some(v) => t.set(&*prop.key, &**v)?,
            None => t.set(&*prop.key, true)?,
        }
    }
    Ok(t)
}

fn iterator(lua: &Lua, items: Vec<LuaMultiValue>) -> LuaResult<LuaFunction> {
    let mut items = items.into_iter();
    lua.create_function_mut(move |_, ()| Ok(items.next().unwrap_or_default()))
}

impl LuaUserData for LuaQuery {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("lang", |_, this, ()| Ok(this.lang.clone()));
        methods.add_method("captures", |_, this, ()| {
            Ok(this
                .query
                .capture_names()
                .iter()
                .map(|s| (*s).to_string())
                .collect::<Vec<_>>())
        });
        methods.add_method("pattern_count", |_, this, ()| {
            Ok(lua_int(this.query.pattern_count()))
        });
        methods.add_method(
            "iter_captures",
            |lua, this, (node, start, end_): (LuaAnyUserData, Option<i64>, Option<i64>)| {
                let target = this.target(&node)?;
                let names = this.query.capture_names();
                let mut items = Vec::new();
                let mut err = None;
                target.snap.for_each_capture(
                    &this.query,
                    target.node(),
                    row_window(start, end_),
                    |m: &QueryMatch<'_, '_>, i| {
                        if err.is_some() {
                            return;
                        }
                        let capture = m.captures()[i];
                        let row = (|| -> LuaResult<LuaMultiValue> {
                            (
                                names[capture.index as usize],
                                LuaNode::new(&target.snap, capture.node),
                                lua_int(m.pattern_index) + 1,
                                metadata(lua, &this.query, m.pattern_index)?,
                            )
                                .into_lua_multi(lua)
                        })();
                        match row {
                            Ok(row) => items.push(row),
                            Err(e) => err = Some(e),
                        }
                    },
                );
                if let Some(e) = err {
                    return Err(e);
                }
                iterator(lua, items)
            },
        );
        methods.add_method(
            "iter_matches",
            |lua, this, (node, start, end_): (LuaAnyUserData, Option<i64>, Option<i64>)| {
                let target = this.target(&node)?;
                let names = this.query.capture_names();
                let mut items = Vec::new();
                let mut err = None;
                target.snap.for_each_match(
                    &this.query,
                    target.node(),
                    row_window(start, end_),
                    |m: &QueryMatch<'_, '_>| {
                        if err.is_some() {
                            return;
                        }
                        let row = (|| -> LuaResult<LuaMultiValue> {
                            let captures = lua.create_table()?;
                            for c in m.captures() {
                                let name = names[c.index as usize];
                                let list =
                                    if let Some(list) = captures.get::<Option<LuaTable>>(name)? {
                                        list
                                    } else {
                                        let list = lua.create_table()?;
                                        captures.set(name, &list)?;
                                        list
                                    };
                                list.push(LuaNode::new(&target.snap, c.node))?;
                            }
                            (
                                lua_int(m.pattern_index) + 1,
                                captures,
                                metadata(lua, &this.query, m.pattern_index)?,
                            )
                                .into_lua_multi(lua)
                        })();
                        match row {
                            Ok(row) => items.push(row),
                            Err(e) => err = Some(e),
                        }
                    },
                );
                if let Some(e) = err {
                    return Err(e);
                }
                iterator(lua, items)
            },
        );
        methods.add_meta_method(LuaMetaMethod::ToString, |_, this, ()| {
            Ok(format!("<query {}>", this.lang))
        });
    }
}

fn kind_arg(kind: &str) -> LuaResult<QueryKind> {
    QueryKind::parse(kind).ok_or_else(|| {
        LuaError::RuntimeError(format!(
            "unknown query kind '{kind}': expected \"highlights\" or \"injections\""
        ))
    })
}

fn load_language(shared: &Shared, lang: &str) -> LuaResult<tree_sitter::Language> {
    shared
        .borrow()
        .language_loader
        .load_language(lang)
        .map(|l| l.language)
        .map_err(|_| LuaError::RuntimeError(format!("unknown language '{lang}'")))
}

fn compile(language: &tree_sitter::Language, lang: &str, source: &str) -> LuaResult<Query> {
    Query::new(language, source)
        .map_err(|e| LuaError::RuntimeError(format!("invalid {lang} query: {e}")))
}

fn editor_query(shared: &Shared, lang: &str, kind: QueryKind) -> LuaResult<Option<Rc<Query>>> {
    let Some(source) = shared.borrow().language_loader.query_source(lang, kind) else {
        return Ok(None);
    };
    let key = (lang.to_string(), kind);
    if let Some((cached_src, query)) = shared.borrow().query_cache.get(&key) {
        if *cached_src == source {
            return Ok(Some(Rc::clone(query)));
        }
    }
    let query = Rc::new(compile(&load_language(shared, lang)?, lang, &source)?);
    shared
        .borrow_mut()
        .query_cache
        .insert(key, (source, Rc::clone(&query)));
    Ok(Some(query))
}

fn trees_of(shared: &Shared, buf: u64) -> Option<Vec<Rc<SyntaxSnapshot>>> {
    let s = shared.borrow();
    let syntax = s.syntax.get(&buf)?;
    Some(
        std::iter::once(Rc::clone(&syntax.host))
            .chain(syntax.injected.iter().cloned())
            .collect(),
    )
}

fn target_byte(
    shared: &Shared,
    snap: &SyntaxSnapshot,
    buf: u64,
    row: Option<i64>,
    col: Option<i64>,
) -> usize {
    let s = shared.borrow();
    if let Some(row) = row {
        snap.text
            .byte_at(lua_index(row - 1), lua_index(col.unwrap_or(0)))
    } else {
        let (row, char_col) = s.bufs.get(&buf).map_or((0, 0), |b| b.meta.cursor);
        snap.text.byte_at(row, snap.text.byte_col(row, char_col))
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn register(lua: &Lua, api: &LuaTable, shared: &Shared) -> LuaResult<()> {
    lua.set_app_data(Rc::clone(shared));
    let ts = lua.create_table()?;

    {
        let sh = Rc::clone(shared);
        let f = lua
            .create_function(move |_, ()| Ok(sh.borrow().language_loader.available_languages()))?;
        ts.set("languages", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(move |_, path: String| {
            Ok(sh
                .borrow()
                .language_loader
                .language_name_for_file(std::path::Path::new(&path)))
        })?;
        ts.set("language_for_path", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(move |_, buf: Option<u64>| {
            let buf = buf_or_current(&sh, buf)?;
            Ok(sh
                .borrow()
                .syntax
                .get(&buf)
                .map(|s| s.host.lang_name.clone()))
        })?;
        ts.set("get_language", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(move |_, buf: Option<u64>| {
            let buf = buf_or_current(&sh, buf)?;
            Ok(sh
                .borrow()
                .syntax
                .get(&buf)
                .map(|s| LuaTree(Rc::clone(&s.host))))
        })?;
        ts.set("get_tree", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(move |_, buf: Option<u64>| {
            let buf = buf_or_current(&sh, buf)?;
            Ok(trees_of(&sh, buf)
                .unwrap_or_default()
                .into_iter()
                .map(LuaTree)
                .collect::<Vec<_>>())
        })?;
        ts.set("get_trees", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(move |_, opts: Option<LuaTable>| {
            let get = |key: &str| -> LuaResult<Option<LuaValue>> {
                opts.as_ref().map(|o| o.get(key)).transpose()
            };
            let buf = buf_or_current(&sh, get("buf")?.and_then(|v| v.as_u64()))?;
            let row = get("row")?.and_then(|v| v.as_i64());
            let col = get("col")?.and_then(|v| v.as_i64());
            let named = get("named")?.and_then(|v| v.as_boolean()).unwrap_or(true);
            let injections = get("injections")?
                .and_then(|v| v.as_boolean())
                .unwrap_or(false);
            let Some(trees) = trees_of(&sh, buf) else {
                return Ok(None);
            };
            let byte = target_byte(&sh, &trees[0], buf, row, col);
            let candidates = if injections { &trees[..] } else { &trees[..1] };
            let best = candidates
                .iter()
                .filter_map(|snap| {
                    let root = snap.root();
                    let node = if named {
                        root.named_descendant_for_byte_range(byte, byte)
                    } else {
                        root.descendant_for_byte_range(byte, byte)
                    }?;
                    let range = node.byte_range();
                    (range.start <= byte && byte <= range.end)
                        .then(|| (range.len(), LuaNode::new(snap, node)))
                })
                .min_by_key(|(len, _)| *len);
            Ok(best.map(|(_, node)| node))
        })?;
        ts.set("get_node", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(move |_, (lang, text): (String, String)| {
            let language = load_language(&sh, &lang)?;
            SyntaxSnapshot::parse_string(lang.clone(), &language, text)
                .map(|snap| LuaTree(Rc::new(snap)))
                .ok_or_else(|| LuaError::RuntimeError(format!("failed to parse {lang} text")))
        })?;
        ts.set("parse", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(move |_, (lang, source): (String, String)| {
            let query = compile(&load_language(&sh, &lang)?, &lang, &source)?;
            Ok(LuaQuery {
                lang,
                query: Rc::new(query),
            })
        })?;
        ts.set("query", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(move |_, (lang, kind): (String, String)| {
            let kind = kind_arg(&kind)?;
            Ok(editor_query(&sh, &lang, kind)?.map(|query| LuaQuery { lang, query }))
        })?;
        ts.set("get_query", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(move |_, (lang, kind): (String, String)| {
            let kind = kind_arg(&kind)?;
            Ok(sh.borrow().language_loader.query_source(&lang, kind))
        })?;
        ts.set("get_query_source", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(
            move |_, (lang, kind, source): (String, String, Option<String>)| {
                let kind = kind_arg(&kind)?;
                if let (Some(source), Ok(language)) = (&source, load_language(&sh, &lang)) {
                    compile(&language, &lang, source)?;
                }
                sh.borrow_mut()
                    .mutations
                    .push(PluginMutation::SetSyntaxQuery { lang, kind, source });
                Ok(())
            },
        )?;
        ts.set("set_query", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(move |_, (buf, lang): (Option<u64>, Option<String>)| {
            let buf = DocumentId::new(buf_or_current(&sh, buf)?);
            sh.borrow_mut()
                .mutations
                .push(PluginMutation::StartSyntax { buf, lang });
            Ok(())
        })?;
        ts.set("start", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(move |_, buf: Option<u64>| {
            let buf = DocumentId::new(buf_or_current(&sh, buf)?);
            sh.borrow_mut()
                .mutations
                .push(PluginMutation::StopSyntax { buf });
            Ok(())
        })?;
        ts.set("stop", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(
            move |lua, (row, col, buf): (Option<i64>, Option<i64>, Option<u64>)| {
                let buf = buf_or_current(&sh, buf)?;
                let out = lua.create_table()?;
                let Some(trees) = trees_of(&sh, buf) else {
                    return Ok(out);
                };
                let byte = target_byte(&sh, &trees[0], buf, row, col);
                for snap in trees.iter().skip(1).chain(trees.iter().take(1)) {
                    let Some(query) = editor_query(&sh, &snap.lang_name, QueryKind::Highlights)?
                    else {
                        continue;
                    };
                    let names = query.capture_names();
                    let mut found: Vec<(std::ops::Range<usize>, usize, LuaNode, &str)> = Vec::new();
                    snap.for_each_match(
                        &query,
                        snap.root(),
                        Some(QueryWindow::Bytes(byte..byte + 1)),
                        |m: &QueryMatch<'_, '_>| {
                            for c in m.captures() {
                                let range = c.node.byte_range();
                                if range.start <= byte && byte < range.end.max(range.start + 1) {
                                    found.push((
                                        range,
                                        m.pattern_index,
                                        LuaNode::new(snap, c.node),
                                        names[c.index as usize],
                                    ));
                                }
                            }
                        },
                    );
                    found.sort_by(|a, b| {
                        b.0.start
                            .cmp(&a.0.start)
                            .then(a.0.end.cmp(&b.0.end))
                            .then(b.1.cmp(&a.1))
                    });
                    for (_, pattern, node, capture) in found {
                        let entry = lua.create_table()?;
                        entry.set("capture", capture)?;
                        entry.set("lang", snap.lang_name.as_str())?;
                        entry.set("node", node)?;
                        entry.set("pattern", lua_int(pattern) + 1)?;
                        out.push(entry)?;
                    }
                }
                Ok(out)
            },
        )?;
        ts.set("captures_at", f)?;
    }

    api.set("treesitter", ts)
}
