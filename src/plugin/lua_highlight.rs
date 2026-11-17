use super::{lua_index, parse_style, LuaSharedState};
use crate::document::DocumentId;
use crate::layer::CellStyle;
use crate::plugin::{HighlightRange, HighlightSpec, PluginMutation};
use mlua::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

type Shared = Rc<RefCell<LuaSharedState>>;

pub(super) fn register(lua: &Lua, api: &LuaTable, shared: &Shared) -> LuaResult<()> {
    let hl = lua.create_table()?;
    register_groups(lua, &hl, shared)?;
    register_namespaces(lua, &hl, shared)?;
    api.set("highlight", hl)
}

fn register_groups(lua: &Lua, hl: &LuaTable, shared: &Shared) -> LuaResult<()> {
    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(move |_, (group, style): (String, LuaTable)| {
            let group = group_name(&group)?;
            let style = cell_style(parse_style(&style)?);
            let mut s = sh.borrow_mut();
            s.user_settings.set_highlight_override(&group, Some(style));
            s.mutations.push(PluginMutation::SetHighlightGroup {
                name: group,
                style: Some(style),
            });
            Ok(())
        })?;
        hl.set("set", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(move |lua, group: String| {
            let style = sh
                .borrow()
                .user_settings
                .syntax_colors
                .as_ref()
                .and_then(|c| c.get_style(&group));
            style.map(|s| style_table(lua, s)).transpose()
        })?;
        hl.set("get", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(move |_, group: Option<String>| {
            let mut s = sh.borrow_mut();
            let mutation = if let Some(group) = group {
                let group = group_name(&group)?;
                s.user_settings.set_highlight_override(&group, None);
                PluginMutation::SetHighlightGroup {
                    name: group,
                    style: None,
                }
            } else {
                s.user_settings.clear_highlight_overrides();
                PluginMutation::ResetHighlightGroups
            };
            s.mutations.push(mutation);
            Ok(())
        })?;
        hl.set("reset", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(move |_, ()| {
            let s = sh.borrow();
            let mut names: Vec<String> = s
                .user_settings
                .syntax_colors
                .as_ref()
                .map_or_default(|c| c.names().map(str::to_string).collect());
            names.sort();
            Ok(names)
        })?;
        hl.set("groups", f)?;
    }

    Ok(())
}

fn register_namespaces(lua: &Lua, hl: &LuaTable, shared: &Shared) -> LuaResult<()> {
    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(
            move |_, (ns, range, spec, opts): (String, LuaValue, LuaValue, Option<LuaTable>)| {
                let ns = namespace(ns)?;
                let (node_buf, range) = parse_range(&range)?;
                let hl = match spec {
                    LuaValue::String(group) => HighlightSpec::Group(group.to_str()?.to_string()),
                    LuaValue::Table(style) => HighlightSpec::Style(parse_style(&style)?),
                    other => {
                        return Err(LuaError::RuntimeError(format!(
                            "rift.highlight.add: hl must be a group name or style table, got {}",
                            other.type_name()
                        )))
                    }
                };
                let opt_buf: Option<u64> =
                    opts.as_ref().map(|o| o.get("buf")).transpose()?.flatten();
                let priority: i32 = opts
                    .as_ref()
                    .map(|o| o.get::<Option<i32>>("priority"))
                    .transpose()?
                    .flatten()
                    .unwrap_or(0);
                let mut s = sh.borrow_mut();
                let buf = opt_buf.or(node_buf).unwrap_or(s.buf_id as u64);
                s.mutations.push(PluginMutation::AddNsHighlight {
                    buf: DocumentId::new(buf),
                    ns,
                    range,
                    hl,
                    priority,
                });
                Ok(())
            },
        )?;
        hl.set("add", f)?;
    }

    {
        let sh = Rc::clone(shared);
        let f = lua.create_function(move |_, (ns, buf): (String, Option<u64>)| {
            let ns = namespace(ns)?;
            sh.borrow_mut()
                .mutations
                .push(PluginMutation::ClearNsHighlights {
                    buf: buf.map(DocumentId::new),
                    ns,
                });
            Ok(())
        })?;
        hl.set("clear", f)?;
    }

    Ok(())
}

fn group_name(group: &str) -> LuaResult<String> {
    let trimmed = group.trim().trim_start_matches('@');
    if trimmed.is_empty() {
        return Err(LuaError::RuntimeError(
            "highlight group name must not be empty".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

fn namespace(ns: String) -> LuaResult<String> {
    if ns.is_empty() {
        return Err(LuaError::RuntimeError(
            "highlight namespace must not be empty".to_string(),
        ));
    }
    Ok(ns)
}

const fn cell_style(s: crate::annotations::presentation::StyleOverride) -> CellStyle {
    CellStyle {
        fg: s.fg,
        bg: s.bg,
        attrs: s.attrs,
    }
}

fn style_table(lua: &Lua, style: CellStyle) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    if let Some(fg) = style.fg {
        t.set("fg", fg.name())?;
    }
    if let Some(bg) = style.bg {
        t.set("bg", bg.name())?;
    }
    for (name, flag) in crate::layer::CellAttrs::NAMED {
        if style.attrs.union(flag) == style.attrs {
            t.set(name, true)?;
        }
    }
    Ok(t)
}

fn parse_range(value: &LuaValue) -> LuaResult<(Option<u64>, HighlightRange)> {
    #[cfg(feature = "treesitter")]
    if let LuaValue::UserData(ud) = value {
        if let Some((buf, bytes)) = super::treesitter_api::node_extent(ud) {
            return Ok((buf, HighlightRange::Bytes(bytes)));
        }
    }
    let LuaValue::Table(t) = value else {
        return Err(LuaError::RuntimeError(format!(
            "highlight range must be a node or {{ start_row, start_col, end_row, end_col }}, got {}",
            value.type_name()
        )));
    };
    let field = |i: usize| -> LuaResult<i64> {
        t.get::<Option<i64>>(i)?.ok_or_else(|| {
            LuaError::RuntimeError(
                "highlight range needs { start_row, start_col, end_row, end_col }".to_string(),
            )
        })
    };
    let (sr, sc, er, ec) = (field(1)?, field(2)?, field(3)?, field(4)?);
    if sr < 1 || er < sr {
        return Err(LuaError::RuntimeError(format!(
            "invalid highlight rows {sr}..{er}: rows are 1-indexed and end >= start"
        )));
    }
    Ok((
        None,
        HighlightRange::Points {
            start: (lua_index(sr - 1), lua_index(sc)),
            end: (lua_index(er - 1), lua_index(ec)),
        },
    ))
}
