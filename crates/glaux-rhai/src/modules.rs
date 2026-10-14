//! The Rhai side of cartridge modules: `import`, its per-runtime cache and
//! cycle detection. Module bytes come from a [`ModuleSource`], and this file
//! knows nothing about where they are stored.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use glaux_modules::{ModuleLoadError, ModuleName, ModulePath, ModuleSource};
use glaux_script::{RequireError, RequireFailure, ScriptError};
use rhai::{Engine, EvalAltResult, Module, ModuleResolver, Position, Scope, Shared};

use crate::error::{failure_error, place, require_error, script_error, syntax_error};
use crate::runtime::Meter;

/// File extension of Rhai modules.
const MODULE_EXTENSION: &str = "rhai";

/// Load state of one module, keyed by its path in [`Modules`].
#[derive(Debug)]
enum ModuleState {
    /// Its top-level code is running, so importing it now is a cycle.
    Loading,
    Loaded(Shared<Module>),
    Failed(ScriptError),
}

/// Every module a runtime has tried to load.
#[derive(Debug, Default)]
struct Modules {
    states: HashMap<String, ModuleState>,
    /// Paths of the modules loading right now, outermost first.
    stack: Vec<String>,
}

/// Resolves `import "name"` to a cartridge module.
///
/// It replaces Rhai's default resolver, which reads the host filesystem:
/// a name is only ever a [`ModuleName`], so nothing outside the
/// cartridge's modules can be named.
pub(crate) struct Resolver {
    source: Rc<dyn ModuleSource>,
    modules: RefCell<Modules>,
    meter: Rc<Meter>,
}

impl Resolver {
    pub(crate) fn new(source: Rc<dyn ModuleSource>, meter: Rc<Meter>) -> Self {
        Self {
            source,
            modules: RefCell::default(),
            meter,
        }
    }

    /// Resolves `name` against the modules already seen. Only a module never
    /// seen before changes state: it is marked loading and pushed on the
    /// stack, and the caller must then hand it to [`finish`](Self::finish).
    fn begin(
        &self,
        name: &str,
        at: Option<String>,
        position: Position,
    ) -> Result<Begin, Box<EvalAltResult>> {
        let module = ModuleName::parse(name).map_err(|reason| {
            let error = RequireError::InvalidName {
                name: name.to_owned(),
                reason,
            };
            require_error(
                RequireFailure {
                    at: at.clone(),
                    error,
                },
                position,
            )
        })?;
        let path = module.path(MODULE_EXTENSION);

        let mut modules = self.modules.borrow_mut();
        match modules.states.get(path.as_str()) {
            Some(ModuleState::Loaded(module)) => return Ok(Begin::Cached(Shared::clone(module))),
            Some(ModuleState::Failed(err)) => return Err(failure_error(err.clone(), position)),
            Some(ModuleState::Loading) => {
                let start = modules
                    .stack
                    .iter()
                    .position(|loading| loading == path.as_str())
                    .unwrap_or(0);
                let mut chain = modules.stack[start..].to_vec();
                chain.push(path.to_string());
                let error = RequireError::Cycle(chain);
                return Err(require_error(RequireFailure { at, error }, position));
            }
            None => {}
        }
        modules
            .states
            .insert(path.to_string(), ModuleState::Loading);
        modules.stack.push(path.to_string());
        Ok(Begin::Fresh(path))
    }

    /// Reads, compiles and runs the module at `path`. Touches no module
    /// state, so it may fail anywhere.
    fn evaluate(
        &self,
        engine: &Engine,
        path: &ModulePath,
        at: Option<String>,
    ) -> Result<Shared<Module>, ScriptError> {
        let bytes = self.source.load(path).map_err(|err| {
            let path = path.to_string();
            let error = match err {
                ModuleLoadError::NotFound => RequireError::NotFound(path),
                ModuleLoadError::Unreadable => RequireError::Unreadable(path),
            };
            ScriptError::Require(Box::new(RequireFailure { at, error }))
        })?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| ScriptError::Syntax(format!("{path}: module is not valid UTF-8")))?;
        let mut ast = engine
            .compile(text)
            .map_err(|err| syntax_error(&err, path.as_str()))?;
        ast.set_source(path.as_str());
        Module::eval_ast_as_new(Scope::new(), &ast, engine)
            .map(Shared::new)
            .map_err(|err| script_error(&err, path.as_str()))
    }

    /// Records how loading the module at `path` ended. Every module
    /// [`begin`](Self::begin) marks as loading passes through here exactly
    /// once, whatever `result` is, so none stays loading.
    fn finish(
        &self,
        was_exhausted: bool,
        path: &ModulePath,
        result: Result<Shared<Module>, ScriptError>,
        position: Position,
    ) -> Result<Shared<Module>, Box<EvalAltResult>> {
        // Nested modules finish first, so the innermost one running out of
        // budget is the one recorded. Exhaustion from before this module
        // started belongs to its caller.
        let ran_out = !was_exhausted && self.meter.exhausted.get();
        if ran_out {
            let mut module = self.meter.module.borrow_mut();
            if module.is_none() {
                *module = Some(path.to_string());
            }
        }
        let result = result.map_err(|err| {
            if ran_out {
                ScriptError::ModuleLimitExceeded(path.to_string())
            } else {
                err
            }
        });

        let mut modules = self.modules.borrow_mut();
        let top = modules.stack.pop();
        debug_assert_eq!(top.as_deref(), Some(path.as_str()), "module stack order");
        let state = match &result {
            Ok(module) => ModuleState::Loaded(Shared::clone(module)),
            Err(err) => ModuleState::Failed(err.clone()),
        };
        modules.states.insert(path.to_string(), state);
        result.map_err(|err| failure_error(err, position))
    }
}

/// What [`Resolver::begin`] found before running anything.
enum Begin {
    Cached(Shared<Module>),
    /// The module was marked loading and must be passed to
    /// [`Resolver::finish`].
    Fresh(ModulePath),
}

impl ModuleResolver for Resolver {
    fn resolve(
        &self,
        engine: &Engine,
        source: Option<&str>,
        name: &str,
        position: Position,
    ) -> Result<Shared<Module>, Box<EvalAltResult>> {
        let at = source.map(|source| place(source, position));
        let path = match self.begin(name, at.clone(), position)? {
            Begin::Cached(module) => return Ok(module),
            Begin::Fresh(path) => path,
        };
        let was_exhausted = self.meter.exhausted.get();
        let result = self.evaluate(engine, &path, at);
        self.finish(was_exhausted, &path, result, position)
    }
}
