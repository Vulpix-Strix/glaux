//! The Luau side of cartridge modules: `require`, its per-runtime cache and
//! cycle detection. Module bytes come from a [`ModuleSource`], and this file
//! knows nothing about where they are stored.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glaux_modules::{ModuleLoadError, ModuleName, ModulePath, ModuleSource};
use glaux_script::{RequireError, RequireFailure};
use mlua::chunk::ChunkMode;
use mlua::{Lua, Value};

use crate::args::{bad_argument, call_site, script_type_name};
use crate::debug;
use crate::runtime::Meter;

/// File extension of Luau modules.
const MODULE_EXTENSION: &str = "luau";

/// Load state of one module, keyed by its path in [`Modules`].
#[derive(Debug)]
pub(crate) enum ModuleState {
    /// Its top-level code is running, so requiring it now is a cycle.
    Loading,
    Loaded(Value),
    Failed(mlua::Error),
}

/// Every module a runtime has tried to load.
#[derive(Debug, Default)]
pub(crate) struct Modules {
    pub(crate) states: HashMap<String, ModuleState>,
    /// Paths of the modules loading right now, outermost first.
    pub(crate) stack: Vec<String>,
}

/// What `require` found before running anything.
enum Begin {
    Cached(Value),
    /// The module was marked loading and must be passed to [`finish`].
    Fresh(ModulePath),
}

/// Replaces the default Luau `require`, which resolves modules on the
/// host filesystem, with one that loads cartridge modules.
pub(crate) fn install(
    lua: &Lua,
    modules: &Rc<RefCell<Modules>>,
    source: &Rc<dyn ModuleSource>,
    meter: &Rc<Meter>,
) -> mlua::Result<()> {
    let modules = Rc::clone(modules);
    let source = Rc::clone(source);
    let meter = Rc::clone(meter);
    let require = lua.create_function(move |lua, name: Value| {
        let path = match begin(lua, &modules, &name)? {
            Begin::Cached(value) => return Ok(value),
            Begin::Fresh(path) => path,
        };
        // Exhaustion from before this module started belongs to its
        // caller, not to the module.
        let was_exhausted = meter.exhausted.get();
        let result = evaluate(lua, &*source, &path);
        finish(&modules, &meter, was_exhausted, &path, result)
    })?;
    lua.globals().raw_set("require", require)
}

/// Resolves `require(name)` against the modules already seen. Only a module
/// never seen before changes state: it is marked loading and pushed on the
/// stack, and the caller must then hand it to [`finish`].
fn begin(lua: &Lua, modules: &RefCell<Modules>, name: &Value) -> mlua::Result<Begin> {
    let Value::String(text) = name else {
        return Err(bad_argument(
            lua,
            "require",
            1,
            &format!("string expected, got {}", script_type_name(name)),
        ));
    };
    let text = text.to_string_lossy();
    let module = ModuleName::parse(&text).map_err(|reason| {
        require_error(
            lua,
            RequireError::InvalidName {
                name: text.clone(),
                reason,
            },
        )
    })?;
    let path = module.path(MODULE_EXTENSION);

    let mut modules = modules.borrow_mut();
    match modules.states.get(path.as_str()) {
        Some(ModuleState::Loaded(value)) => return Ok(Begin::Cached(value.clone())),
        Some(ModuleState::Failed(err)) => return Err(err.clone()),
        Some(ModuleState::Loading) => {
            let start = modules
                .stack
                .iter()
                .position(|loading| loading == path.as_str())
                .unwrap_or(0);
            let mut chain = modules.stack[start..].to_vec();
            chain.push(path.to_string());
            drop(modules);
            return Err(require_error(lua, RequireError::Cycle(chain)));
        }
        None => {}
    }
    modules
        .states
        .insert(path.to_string(), ModuleState::Loading);
    modules.stack.push(path.to_string());
    Ok(Begin::Fresh(path))
}

/// Reads and runs the module at `path`, returning its first result, or
/// `true` if it returned nothing. Touches no module state, so it may fail
/// anywhere.
fn evaluate(lua: &Lua, source: &dyn ModuleSource, path: &ModulePath) -> mlua::Result<Value> {
    let bytes = source.load(path).map_err(|err| {
        let path = path.to_string();
        let error = match err {
            ModuleLoadError::NotFound => RequireError::NotFound(path),
            ModuleLoadError::Unreadable => RequireError::Unreadable(path),
        };
        require_error(lua, error)
    })?;
    let chunk = lua
        .load(&bytes)
        .set_name(format!("={path}"))
        .set_mode(ChunkMode::Text)
        .into_function()?;
    debug::chunk_loaded(lua, path.as_str(), &bytes, &chunk);
    let value: Value = chunk.call(())?;
    Ok(if value.is_nil() {
        Value::Boolean(true)
    } else {
        value
    })
}

/// Records how loading the module at `path` ended. Every module [`begin`]
/// marks as loading passes through here exactly once, whatever `result` is,
/// so none stays loading.
fn finish(
    modules: &RefCell<Modules>,
    meter: &Meter,
    was_exhausted: bool,
    path: &ModulePath,
    result: mlua::Result<Value>,
) -> mlua::Result<Value> {
    let mut modules = modules.borrow_mut();
    let top = modules.stack.pop();
    debug_assert_eq!(top.as_deref(), Some(path.as_str()), "module stack order");
    let state = match &result {
        Ok(value) => ModuleState::Loaded(value.clone()),
        Err(err) => ModuleState::Failed(err.clone()),
    };
    modules.states.insert(path.to_string(), state);

    // Nested modules finish first, so the innermost one running out of
    // budget is the one recorded.
    if !was_exhausted && meter.exhausted.get() {
        let mut module = meter.module.borrow_mut();
        if module.is_none() {
            *module = Some(path.to_string());
        }
    }
    result
}

/// A [`RequireFailure`] located at the script calling `require`.
fn require_error(lua: &Lua, error: RequireError) -> mlua::Error {
    mlua::Error::external(RequireFailure {
        at: call_site(lua),
        error,
    })
}
