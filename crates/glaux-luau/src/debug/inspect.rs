//! Reading the call stack and variables of a stopped Luau VM.
//!
//! Everything here reads raw values. No metamethod, `tostring` or any other
//! script code runs, so inspecting cannot change the script's behavior or
//! spend its budget.

use std::ffi::{CStr, c_char, c_int};

use glaux_script::debug::{Frame, Variable};
use mlua::ffi::{self, lua_Debug, lua_State};

/// Entries listed for one table.
const MAX_ENTRIES: usize = 64;

/// Longest string shown, in bytes.
const MAX_TEXT: usize = 40;

/// Stack slots an inspection may need beyond what the hook was given.
const SLOTS: c_int = 8;

/// The call stack, innermost first. A frame is script code when its chunk
/// name is `=` followed by one of `known`.
///
/// # Safety
/// `l` must be a live state stopped inside a debug hook.
pub(super) unsafe fn frames(l: *mut lua_State, known: &dyn Fn(&str) -> bool) -> Vec<Frame> {
    // SAFETY: the caller guarantees `l` is live, and `lua_getinfo` only
    // reads the call frame at each level below the depth it reports.
    unsafe {
        (0..ffi::lua_stackdepth(l))
            .map_while(|level| {
                let mut ar: lua_Debug = std::mem::zeroed();
                if ffi::lua_getinfo(l, level, c"snl".as_ptr(), &mut ar) == 0 {
                    return None;
                }
                let source = text(ar.source);
                let path = source
                    .strip_prefix('=')
                    .filter(|path| known(path))
                    .map(str::to_owned);
                let function = if text(ar.what) == "main" {
                    "main".to_owned()
                } else {
                    text(ar.name)
                };
                Some(Frame {
                    has_variables: path.is_some(),
                    function,
                    line: (path.is_some() && ar.currentline > 0).then_some(ar.currentline as u32),
                    path,
                })
            })
            .collect()
    }
}

/// Visits the locals and then the upvalues of the function at `level`, with
/// each value on top of the stack. `visit` gets the variable's index, which
/// is what [`expand`] takes, and its name, and returns whether to go on.
///
/// # Safety
/// `l` must be a live state stopped inside a debug hook, and `visit` must
/// leave the stack as it found it.
unsafe fn each_variable(
    l: *mut lua_State,
    level: c_int,
    visit: &mut dyn FnMut(usize, &str) -> bool,
) {
    // SAFETY: the caller guarantees `l` is live. Every push below is popped
    // before returning and `lua_checkstack` reserves the room.
    unsafe {
        ffi::lua_checkstack(l, SLOTS);
        let mut index = 0;
        for n in 1.. {
            let name = ffi::lua_getlocal(l, level, n);
            if name.is_null() {
                break;
            }
            let name = text(name);
            // Compiler temporaries are named like "(for state)".
            if name.starts_with('(') {
                ffi::lua_settop(l, -2);
                continue;
            }
            let more = visit(index, &name);
            ffi::lua_settop(l, -2);
            if !more {
                return;
            }
            index += 1;
        }
        let mut ar: lua_Debug = std::mem::zeroed();
        if ffi::lua_getinfo(l, level, c"f".as_ptr(), &mut ar) == 0 {
            return;
        }
        let function = ffi::lua_gettop(l);
        for n in 1.. {
            let name = ffi::lua_getupvalue(l, function, n);
            if name.is_null() {
                break;
            }
            let name = text(name);
            let more = visit(index, &name);
            ffi::lua_settop(l, -2);
            if !more {
                break;
            }
            index += 1;
        }
        ffi::lua_settop(l, function - 1);
    }
}

/// The variables of the function at `level`.
///
/// # Safety
/// As [`each_variable`].
pub(super) unsafe fn variables(l: *mut lua_State, level: c_int) -> Vec<Variable> {
    let mut out = Vec::new();
    // SAFETY: the visitor only reads the value on top of the stack.
    unsafe {
        each_variable(l, level, &mut |_, name| {
            out.push(Variable {
                name: name.to_owned(),
                value: render(l, -1),
                expandable: is_table(l, -1),
            });
            true
        });
    }
    out
}

/// The entries of the table at the end of `path`: the first index picks a
/// variable of the function at `level`, each further one an entry of the
/// table before it. `None` when the path no longer leads to a table.
///
/// # Safety
/// As [`each_variable`].
pub(super) unsafe fn expand(
    l: *mut lua_State,
    level: c_int,
    path: &[usize],
) -> Option<Vec<Variable>> {
    let (&first, rest) = path.split_first()?;
    let mut found = None;
    // SAFETY: `descend` leaves the stack as it found it.
    unsafe {
        each_variable(l, level, &mut |index, _| {
            if index == first {
                found = Some(descend(l, rest));
                return false;
            }
            true
        });
    }
    found.flatten()
}

/// Follows `path` from the table on top of the stack and lists the entries
/// it ends on.
unsafe fn descend(l: *mut lua_State, path: &[usize]) -> Option<Vec<Variable>> {
    // SAFETY: each step pushes one value that is popped before the next
    // return, and only tables are iterated.
    unsafe {
        if !is_table(l, -1) {
            return None;
        }
        let top = ffi::lua_gettop(l);
        let mut at = ffi::lua_absindex(l, -1);
        for &entry in path {
            if !push_entry(l, at, entry) || !is_table(l, -1) {
                ffi::lua_settop(l, top);
                return None;
            }
            at = ffi::lua_gettop(l);
        }
        let mut out = Vec::new();
        ffi::lua_pushnil(l);
        while ffi::lua_next(l, at) != 0 {
            out.push(Variable {
                name: key_text(l, -2),
                value: render(l, -1),
                expandable: is_table(l, -1),
            });
            ffi::lua_settop(l, -2);
            if out.len() == MAX_ENTRIES {
                ffi::lua_settop(l, -2);
                break;
            }
        }
        ffi::lua_settop(l, top);
        Some(out)
    }
}

/// Pushes the value of the `n`th entry of the table at `table`, in the
/// order `lua_next` visits them. Returns `false`, pushing nothing, past
/// the end.
unsafe fn push_entry(l: *mut lua_State, table: c_int, n: usize) -> bool {
    // SAFETY: `table` is a table index below the top, and a failed
    // iteration leaves nothing behind.
    unsafe {
        ffi::lua_pushnil(l);
        let mut seen = 0;
        while ffi::lua_next(l, table) != 0 {
            if seen == n {
                ffi::lua_replace(l, -2);
                return true;
            }
            seen += 1;
            ffi::lua_settop(l, -2);
        }
        false
    }
}

unsafe fn is_table(l: *mut lua_State, index: c_int) -> bool {
    // SAFETY: `index` is a valid stack index by the caller's contract.
    unsafe { ffi::lua_type(l, index) == ffi::LUA_TTABLE && has_entries(l, index) }
}

/// Whether the table at `index` has any entry. Empty tables are shown as
/// `{}` and cannot be expanded.
unsafe fn has_entries(l: *mut lua_State, index: c_int) -> bool {
    // SAFETY: a raw `lua_next` from nil reads the first entry, if any, and
    // the two slots it pushes are popped.
    unsafe {
        let table = ffi::lua_absindex(l, index);
        ffi::lua_pushnil(l);
        if ffi::lua_next(l, table) == 0 {
            return false;
        }
        ffi::lua_settop(l, -3);
        true
    }
}

/// A table key as written in a table constructor.
unsafe fn key_text(l: *mut lua_State, index: c_int) -> String {
    // SAFETY: `index` is a valid stack index by the caller's contract.
    unsafe {
        if ffi::lua_type(l, index) == ffi::LUA_TSTRING {
            let key = string(l, index, usize::MAX);
            if is_identifier(&key) {
                return key;
            }
            return format!("[\"{}\"]", clip(&key));
        }
        format!("[{}]", render(l, index))
    }
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A short rendering of the value at `index`, free of side effects.
///
/// # Safety
/// `index` must be a valid stack index.
pub(super) unsafe fn render(l: *mut lua_State, index: c_int) -> String {
    // SAFETY: only raw reads of the value at `index`.
    unsafe {
        match ffi::lua_type(l, index) {
            ffi::LUA_TNIL => "nil".to_owned(),
            ffi::LUA_TBOOLEAN => (ffi::lua_toboolean(l, index) != 0).to_string(),
            ffi::LUA_TNUMBER => number(ffi::lua_tonumberx(l, index, std::ptr::null_mut())),
            ffi::LUA_TSTRING => format!("\"{}\"", clip(&string(l, index, MAX_TEXT + 1))),
            ffi::LUA_TTABLE => if has_entries(l, index) { "{..}" } else { "{}" }.to_owned(),
            ffi::LUA_TFUNCTION => "function".to_owned(),
            ffi::LUA_TVECTOR => {
                let v = ffi::lua_tovector(l, index);
                if v.is_null() {
                    return "vector".to_owned();
                }
                let [x, y, z] = [0, 1, 2].map(|i| number(f64::from(*v.add(i))));
                format!("({x}, {y}, {z})")
            }
            other => text(ffi::lua_typename(l, other)),
        }
    }
}

/// Integers without a fraction, like Luau's own `tostring`.
fn number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// The bytes of the string at `index`, at most `limit` of them.
unsafe fn string(l: *mut lua_State, index: c_int, limit: usize) -> String {
    // SAFETY: the value is a string, so `lua_tolstring` converts nothing and
    // returns `len` valid bytes.
    unsafe {
        let mut len = 0;
        let bytes = ffi::lua_tolstring(l, index, &mut len);
        if bytes.is_null() {
            return String::new();
        }
        String::from_utf8_lossy(std::slice::from_raw_parts(bytes.cast(), len.min(limit)))
            .into_owned()
    }
}

/// `text` cut to [`MAX_TEXT`] bytes at a character boundary.
fn clip(text: &str) -> String {
    if text.len() <= MAX_TEXT {
        return text.to_owned();
    }
    let end = (0..=MAX_TEXT)
        .rev()
        .find(|&end| text.is_char_boundary(end))
        .unwrap_or(0);
    format!("{}...", &text[..end])
}

unsafe fn text(c: *const c_char) -> String {
    if c.is_null() {
        return String::new();
    }
    // SAFETY: Luau returns NUL-terminated names that outlive this call.
    unsafe { CStr::from_ptr(c).to_string_lossy().into_owned() }
}
