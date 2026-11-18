use super::{LuaHost, LuaSharedState};
use crate::time::Instant;
use mlua::prelude::*;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::time::Duration;

type Shared = Rc<RefCell<LuaSharedState>>;

const MIN_INTERVAL: Duration = Duration::from_millis(1);

struct Timer {
    interval: Option<Duration>,
    callback: LuaFunction,
    plugin: Option<Rc<str>>,
}

#[derive(Default)]
pub(super) struct Timers {
    next_id: u64,
    queue: BTreeMap<(Instant, u64), Timer>,
    deadlines: HashMap<u64, Instant>,
}

impl Timers {
    fn insert(&mut self, deadline: Instant, timer: Timer) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.schedule(id, deadline, timer);
        id
    }

    fn schedule(&mut self, id: u64, deadline: Instant, timer: Timer) {
        self.deadlines.insert(id, deadline);
        self.queue.insert((deadline, id), timer);
    }

    fn remove(&mut self, id: u64) -> Option<Timer> {
        let deadline = self.deadlines.remove(&id)?;
        self.queue.remove(&(deadline, id))
    }

    pub(super) fn remove_owned(&mut self, plugin: &str) {
        let deadlines = &mut self.deadlines;
        self.queue.retain(|(_, id), timer| {
            let owned = timer.plugin.as_deref() == Some(plugin);
            if owned {
                deadlines.remove(id);
            }
            !owned
        });
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.queue
            .first_key_value()
            .map(|(&(deadline, _), _)| deadline)
    }

    fn due(&self, now: Instant) -> Vec<u64> {
        self.queue
            .range(..=(now, u64::MAX))
            .map(|(&(_, id), _)| id)
            .collect()
    }

    fn take(&mut self, id: u64, now: Instant) -> Option<(LuaFunction, Option<Rc<str>>)> {
        let deadline = self.deadlines.remove(&id)?;
        let timer = self.queue.remove(&(deadline, id))?;
        let fired = (timer.callback.clone(), timer.plugin.clone());
        if let Some(interval) = timer.interval {
            let next = deadline
                .checked_add(interval)
                .filter(|next| *next > now)
                .or_else(|| now.checked_add(interval));
            if let Some(next) = next {
                self.schedule(id, next, timer);
            }
        }
        Some(fired)
    }

    fn owners(&self) -> BTreeMap<&str, usize> {
        let mut owners = BTreeMap::new();
        for plugin in self
            .queue
            .values()
            .filter_map(|timer| timer.plugin.as_deref())
        {
            *owners.entry(plugin).or_default() += 1;
        }
        owners
    }
}

fn schedule(
    shared: &RefCell<LuaSharedState>,
    delay: Duration,
    interval: Option<Duration>,
    callback: LuaFunction,
) -> LuaResult<u64> {
    let deadline = Instant::now()
        .checked_add(delay)
        .ok_or_else(|| LuaError::RuntimeError("timer delay is too large".into()))?;
    let mut s = shared.borrow_mut();
    let plugin = s.current_plugin.as_deref().map(Rc::from);
    Ok(s.timers.insert(
        deadline,
        Timer {
            interval,
            callback,
            plugin,
        },
    ))
}

pub(super) fn register(lua: &Lua, api: &LuaTable, shared: &Shared) -> LuaResult<()> {
    let sh = Rc::clone(shared);
    api.set(
        "defer",
        lua.create_function(move |_, callback: LuaFunction| {
            schedule(&sh, Duration::ZERO, None, callback)
        })?,
    )?;

    let sh = Rc::clone(shared);
    api.set(
        "timer",
        lua.create_function(
            move |_, (ms, callback, opts): (u64, LuaFunction, Option<LuaTable>)| {
                let repeat = match opts {
                    Some(opts) => opts.get::<Option<bool>>("repeat")?.unwrap_or(false),
                    None => false,
                };
                let delay = Duration::from_millis(ms);
                if repeat {
                    let interval = delay.max(MIN_INTERVAL);
                    schedule(&sh, interval, Some(interval), callback)
                } else {
                    schedule(&sh, delay, None, callback)
                }
            },
        )?,
    )?;

    let sh = Rc::clone(shared);
    api.set(
        "timer_stop",
        lua.create_function(move |_, id: u64| Ok(sh.borrow_mut().timers.remove(id).is_some()))?,
    )?;

    let sh = Rc::clone(shared);
    lua.globals().set(
        "_rift_plugin_timers",
        lua.create_function(move |lua, ()| {
            let counts = lua.create_table()?;
            for (plugin, count) in sh.borrow().timers.owners() {
                counts.set(plugin, count)?;
            }
            Ok(counts)
        })?,
    )
}

impl LuaHost {
    pub(in crate::plugin) fn next_timer_deadline(&self) -> Option<Instant> {
        self.shared.borrow().timers.next_deadline()
    }

    pub(in crate::plugin) fn fire_due_timers(&self, now: Instant) -> bool {
        let due = self.shared.borrow().timers.due(now);
        let mut fired = false;
        for id in due {
            let Some((callback, plugin)) = self.shared.borrow_mut().timers.take(id, now) else {
                continue;
            };
            let plugin = plugin.as_deref();
            if self.is_disabled(plugin) {
                continue;
            }
            fired = true;
            if let Err(error) = self.call_plugin(plugin, Self::AUTO_DISPATCH_BUDGET, || {
                callback.call::<()>(())
            }) {
                self.report_error(plugin, "timer", &error);
            }
        }
        fired
    }
}
