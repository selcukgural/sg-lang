//! Functions of C in the interpreter.
//!
//! The interpreter cannot call a C function whose signature it learns only when the program
//! is read, and the language takes no dependency (such as libffi) to do it. So, before the
//! program runs, it writes a small C file with a wrapper for each function of C the program
//! calls, all with one signature, `void w(void **args, void *ret)`, builds it as a shared
//! library with the C compiler (a DLL on Windows), linked with the libraries the `extern` blocks
//! name, and opens it. A call writes each argument into a slot of its own, as the C type it is, and hands
//! the wrapper the addresses of the slots.
//!
//! The library is built in a folder of its own that only this user can open, made new for
//! each run and removed once the library is open: a library left where another user could
//! write would run that user's code. Nothing is kept between runs. On Windows a DLL that is
//! loaded cannot be deleted: its folder is removed by the next interpreter that starts.

use std::collections::{BTreeSet, HashMap};
use std::ffi::{c_char, c_int, c_void};
use std::path::{Path, PathBuf};

use crate::ast::{short_name, Callee, ExprKind, Program};
use crate::diag::Diag;
use crate::types::{FloatTy, IntTy, Ty};

/// What a wrapper is: it puts the program's `errno` in place (and on Windows its last error of
/// the system, the second of the two numbers), reads each argument from the slot whose address it
/// is given, calls the function, writes what it gives into `ret`, and keeps both as the call left
/// them.
pub type Wrapper = unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *mut c_int);

/// What gives the address of a variable of C, or of a function of C.
type Address = unsafe extern "C" fn() -> *mut c_void;

/// What calls a function of C through its address (a `CFunc`): the address first, then as a
/// wrapper.
pub type Through = unsafe extern "C" fn(*mut c_void, *mut *mut c_void, *mut c_void, *mut c_int);

/// What C calls for an `export` function of the program: the number of the function, its
/// name, the addresses of its arguments and where what it gives goes. The interpreter's.
pub type Enter = unsafe extern "C" fn(c_int, *const c_char, *mut *mut c_void, *mut c_void);

/// What the library is given the interpreter's entry with.
type SetEnter = unsafe extern "C" fn(Enter);

/// The wrappers of the functions of C a program calls, by their full name in the program, and
/// the addresses of the variables of C it uses.
#[derive(Default)]
pub struct Library {
    calls: HashMap<String, Wrapper>,
    vars: HashMap<String, usize>,
    /// What calls through a `CFunc`, by the C types of the call (see `through_key`).
    throughs: HashMap<String, Through>,
    /// The address of each function whose name is a `CFunc` value: an `export` function's
    /// stub, or a function of C.
    addresses: HashMap<String, usize>,
    /// The `export` functions, by number, with what they take and give.
    exports: Vec<(String, Vec<Ty>, Ty)>,
}

/// One value as C takes it: eight bytes, enough for any argument, and aligned for each.
pub type Slot = u64;

/// What the C file of the wrappers starts with besides its declarations: what is seen from
/// outside the library (on Windows only what is exported from a DLL), and how a call of C takes
/// the program's `errno` and gives it back, `sg_e[0]`, with, on Windows, the system's last error
/// (`GetLastError`), `sg_e[1]`. The functions of Windows are declared under names of the wrappers'
/// own, so that a C file of the program that includes `windows.h` meets no second declaration.
/// The libraries of the system are those compiled code names (see `emit_c`), and with them the
/// functions of C's `printf` and its kin, which Microsoft's C library has only in a header.
const WINDOWS_HEAD: &str = "#ifdef _WIN32\n\
#pragma comment(lib, \"legacy_stdio_definitions\")\n\
#pragma comment(lib, \"kernel32\")\n\
#pragma comment(lib, \"advapi32\")\n\
#pragma comment(lib, \"ws2_32\")\n\
#pragma comment(lib, \"bcrypt\")\n\
#pragma comment(lib, \"ntdll\")\n\
#define SG_W __declspec(dllexport)\n\
unsigned long sg_get_last_error(void) SG_C(\"GetLastError\");\n\
void sg_set_last_error(unsigned long code) SG_C(\"SetLastError\");\n\
#define SG_ENTER_C(e) (errno = (e)[0], sg_set_last_error((unsigned long)(e)[1]))\n\
#define SG_LEAVE_C(e) ((e)[0] = errno, (e)[1] = (int)sg_get_last_error())\n\
#else\n\
#define SG_W\n\
#define SG_ENTER_C(e) (errno = (e)[0])\n\
#define SG_LEAVE_C(e) ((e)[0] = errno)\n\
#endif\n";

/// On Windows, C's output and standard error in binary mode, as compiled code sets them
/// (`sg_start`): a `\n` that a function of C writes is written as it is, not as `\r\n`. The
/// interpreter writes through Windows itself, which this does not touch. Elsewhere nothing.
fn binary_streams() {
    #[cfg(windows)]
    {
        extern "C" {
            fn _setmode(fd: std::ffi::c_int, mode: std::ffi::c_int) -> std::ffi::c_int;
        }
        const O_BINARY: std::ffi::c_int = 0x8000;
        // SAFETY: 1 and 2 are the C library's descriptors of the output and the standard error.
        unsafe {
            _setmode(1, O_BINARY);
            _setmode(2, O_BINARY);
        }
    }
}

/// What wraps one function of C (or gives the address of one variable), in the C file of the
/// wrappers.
#[allow(clippy::too_many_arguments)]
fn wrapper_text(prog: &Program, name: &str, number: usize, addressed: bool, call: Option<&[Ty]>, types: &mut BTreeSet<String>, declared: &mut BTreeSet<String>, head: &mut String, text: &mut String) {
    let in_file = foreign_of(prog, name).path.is_some();
    let c_name = short_name(name);
    if let Some(ty) = prog.cvar_types.get(name) {
        type_text(ty, &prog.plain, types, head);
        // A variable of the C file is seen by its name; one of a library under a name of
        // our own, bound to its C name, so that it clashes with no header.
        let place = match in_file {
            true => c_name.to_string(),
            false => {
                if declared.insert(format!("v {c_name}")) {
                    let (item, length) = match ty {
                        Ty::Array(item, length) => (c_type(item), format!("[{length}]")),
                        other => (c_type(other), String::new()),
                    };
                    head.push_str(&format!("extern {item} sg_cv_{c_name}{length} SG_C(\"{c_name}\");\n"));
                }
                format!("sg_cv_{c_name}")
            }
        };
        text.push_str(&format!("SG_W void *sg_v{number}(void) {{ return (void*)&{place}; }}\n"));
        return;
    }
    let (declared_params, ret) = &prog.foreign[name];
    // A function declared with `...` is called with the types of one call.
    let params = call.unwrap_or(declared_params);
    for ty in params.iter().chain([ret]) {
        type_text(ty, &prog.plain, types, head);
    }
    let callee = match in_file {
        true => c_name.to_string(),
        false => {
            if declared.insert(format!("f {c_name}")) {
                let mut list: Vec<String> = declared_params.iter().map(c_type).collect();
                if prog.variadic.contains(name) {
                    list.push("...".to_string());
                }
                let list = if list.is_empty() { "void".to_string() } else { list.join(", ") };
                head.push_str(&format!("{} sg_c_{c_name}({list}) SG_C(\"{c_name}\");\n", c_type(ret)));
            }
            format!("sg_c_{c_name}")
        }
    };
    let types: Vec<String> = params.iter().map(c_type).collect();
    // A struct given by value lies elsewhere, and its slot holds its address; so does the room
    // for one given back, in the slot after the arguments.
    let args: Vec<String> = types
        .iter()
        .zip(params)
        .enumerate()
        .map(|(at, (ty, param))| match param {
            Ty::Struct(_) => format!("**({ty}**)sg_a[{at}]"),
            _ => format!("*({ty}*)sg_a[{at}]"),
        })
        .collect();
    let call = format!("{callee}({})", args.join(", "));
    let body = match ret {
        Ty::Void => format!("(void)sg_r; {call};"),
        Ty::Struct(_) => format!("(void)sg_r; **({}**)sg_a[{}] = {call};", c_type(ret), params.len()),
        _ => format!("*({}*)sg_r = {call};", c_type(ret)),
    };
    text.push_str(&format!("SG_W void sg_w{number}(void **sg_a, void *sg_r, int *sg_e) {{ (void)sg_a; SG_ENTER_C(sg_e); {body} SG_LEAVE_C(sg_e); }}\n"));
    // Its name is a `CFunc` somewhere: the address of the function itself.
    if addressed {
        text.push_str(&format!("SG_W void *sg_f{number}(void) {{ return (void*)&{callee}; }}\n"));
    }
}

/// The C types of a call, which name the wrapper that calls through a `CFunc` of them.
pub fn through_key(params: &[Ty], ret: &Ty) -> String {
    let types: Vec<String> = params.iter().map(c_type).collect();
    format!("{}({})", c_type(ret), types.join(","))
}

/// What names the wrapper of one call of a function of C declared with `...`: its name and
/// the C types the call gives it.
pub fn variadic_key(name: &str, params: &[Ty], ret: &Ty) -> String {
    format!("{name}#{}", through_key(params, ret))
}

/// What calls a function of C of these types through its address, numbered `number`.
fn through_text(params: &[Ty], ret: &Ty, number: usize, plain: &crate::raw::PlainTypes, types: &mut BTreeSet<String>, head: &mut String, text: &mut String) {
    for ty in params.iter().chain([ret]) {
        type_text(ty, plain, types, head);
    }
    let c_types: Vec<String> = params.iter().map(c_type).collect();
    let list = if c_types.is_empty() { "void".to_string() } else { c_types.join(", ") };
    let args: Vec<String> = c_types
        .iter()
        .zip(params)
        .enumerate()
        .map(|(at, (ty, param))| match param {
            Ty::Struct(_) => format!("**({ty}**)sg_a[{at}]"),
            _ => format!("*({ty}*)sg_a[{at}]"),
        })
        .collect();
    let call = format!("(({} (*)({list}))sg_p)({})", c_type(ret), args.join(", "));
    let body = match ret {
        Ty::Void => format!("(void)sg_r; {call};"),
        Ty::Struct(_) => format!("(void)sg_r; **({}**)sg_a[{}] = {call};", c_type(ret), params.len()),
        _ => format!("*({}*)sg_r = {call};", c_type(ret)),
    };
    text.push_str(&format!("SG_W void sg_t{number}(void *sg_p, void **sg_a, void *sg_r, int *sg_e) {{ (void)sg_a; SG_ENTER_C(sg_e); {body} SG_LEAVE_C(sg_e); }}\n"));
}

/// Whether C files of the program are told about this `export` function before them: not
/// when it takes or gives a struct by value, whose C name is SG's own.
pub fn declared_to_c(params: &[Ty], ret: &Ty) -> bool {
    !params.iter().chain([ret]).any(|ty| matches!(ty, Ty::Struct(_)))
}

/// What C calls for an `export` function: a function of its name and types that hands the
/// addresses of its arguments to the interpreter, numbered `number`.
#[allow(clippy::too_many_arguments)]
fn export_text(name: &str, params: &[Ty], ret: &Ty, number: usize, plain: &crate::raw::PlainTypes, types: &mut BTreeSet<String>, head: &mut String, text: &mut String) {
    for ty in params.iter().chain([ret]) {
        type_text(ty, plain, types, head);
    }
    let c_name = short_name(name);
    // A value of SG that C holds is an address C does not look into.
    let export_type = |ty: &Ty| match ty {
        Ty::Read(_) => "const void*".to_string(),
        other => c_type(other),
    };
    let c_types: Vec<String> = params.iter().map(export_type).collect();
    let named: Vec<String> = c_types.iter().enumerate().map(|(at, ty)| format!("{ty} sg_a{at}")).collect();
    let list = if named.is_empty() { "void".to_string() } else { named.join(", ") };
    if declared_to_c(params, ret) {
        let bare = if c_types.is_empty() { "void".to_string() } else { c_types.join(", ") };
        head.push_str(&format!("SG_W {} {c_name}({bare});\n", c_type(ret)));
    }
    let mut addresses: Vec<String> = (0..params.len()).map(|at| format!("(void*)&sg_a{at}")).collect();
    addresses.push("0".to_string());
    let (room, back) = match ret {
        Ty::Void => (String::new(), ("0".to_string(), String::new())),
        _ => (format!(" {} sg_r;", c_type(ret)), ("(void*)&sg_r".to_string(), " return sg_r;".to_string())),
    };
    text.push_str(&format!(
        "SG_W {} {c_name}({list}) {{{room} void *sg_args[] = {{ {} }}; sg_enter({number}, \"{c_name}\", sg_args, {});{} }}\n",
        c_type(ret),
        addresses.join(", "),
        back.0,
        back.1
    ));
}

/// Where the function or the variable of C named `name` is declared.
fn foreign_of<'p>(prog: &'p Program, name: &str) -> &'p crate::ast::Foreign {
    match prog.cvars.iter().find(|var| var.name == name) {
        Some(var) => &var.foreign,
        None => prog.funcs.iter().find(|func| func.name == name).and_then(|func| func.foreign.as_ref()).expect("a function of C"),
    }
}

/// The libraries a program that uses C links, and the C files it compiles. The libraries of a
/// module are linked when anything of C in it is used: a C file of the module may need them
/// (`extern "curl" { }`). A C file is compiled when something its block declares is used.
pub fn links<'p>(prog: &'p Program, used: &[String]) -> (Vec<&'p crate::ast::Foreign>, BTreeSet<String>) {
    let modules: BTreeSet<&str> = used.iter().map(|name| crate::ast::module_of(name)).collect();
    let libs = prog.blocks.iter().filter(|(module, foreign)| modules.contains(module.as_str()) && !foreign.libs.is_empty()).map(|(_, foreign)| foreign).collect();
    let files = used.iter().filter_map(|name| foreign_of(prog, name).path.clone()).collect();
    (libs, files)
}

/// The text a C file is taken into a unit with: as a string of C.
pub fn include_line(path: &str) -> String {
    let mut quoted = String::new();
    for c in path.chars() {
        match c {
            '"' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            other => quoted.push(other),
        }
    }
    format!("#include \"{quoted}\"\n")
}

impl Library {
    /// Builds and opens the wrappers of the functions of C that the program calls, in the
    /// bodies of its functions and methods (and of its tests, with `tests`), and the
    /// addresses of the variables of C it uses. A program that uses none gets an empty library
    /// and nothing is built.
    pub fn open(prog: &Program, tests: bool) -> Result<Library, Diag> {
        let used = used(prog, tests);
        let values = c_values(prog, tests);
        if used.is_empty() && values.throughs.is_empty() && !values.exports {
            return Ok(Library::default());
        }
        let (blocks, files) = links(prog, &used);
        let libs = crate::link::choose(prog, &blocks)?;
        // As in compiled code: the declarations, then the C files, then what reaches into them.
        let mut head = String::from("#if defined(__linux__) && !defined(_GNU_SOURCE)\n#define _GNU_SOURCE\n#endif\n");
        head.push_str(crate::raw::C_ALIGNED);
        head.push_str("#include <stdint.h>\n#include <errno.h>\n#ifdef __APPLE__\n#define SG_C(name) __asm__(\"_\" name)\n#else\n#define SG_C(name) __asm__(name)\n#endif\n");
        head.push_str(WINDOWS_HEAD);
        // The structs and the types of functions C is given or gives, each defined once.
        let mut types: BTreeSet<String> = BTreeSet::new();
        // A C function declared in two modules is one function of C.
        let mut declared: BTreeSet<String> = BTreeSet::new();
        let (mut before, mut after) = (String::new(), String::new());
        // A function declared with `...` has a wrapper for each list of types it is called with,
        // numbered after the others.
        let calls = variadic_calls(prog, tests);
        let fixed: Vec<&String> = used.iter().filter(|name| !prog.variadic.contains(*name)).collect();
        for (number, name) in fixed.iter().enumerate() {
            let text = if foreign_of(prog, name).path.is_some() { &mut after } else { &mut before };
            wrapper_text(prog, name, number, values.addressed.contains(*name), None, &mut types, &mut declared, &mut head, text);
        }
        for (number, (name, params)) in calls.values().enumerate() {
            let text = if foreign_of(prog, name).path.is_some() { &mut after } else { &mut before };
            wrapper_text(prog, name, fixed.len() + number, false, Some(params), &mut types, &mut declared, &mut head, text);
        }
        for (number, (params, ret)) in values.throughs.values().enumerate() {
            through_text(params, ret, number, &prog.plain, &mut types, &mut head, &mut after);
        }
        let exports: Vec<(String, Vec<Ty>, Ty)> = prog.exported.iter().map(|(name, (params, ret))| (name.clone(), params.clone(), ret.clone())).collect();
        if !exports.is_empty() {
            after.push_str("static void (*sg_enter)(int, const char*, void**, void*);\nSG_W void sg_set_enter(void (*enter)(int, const char*, void**, void*)) { sg_enter = enter; }\n");
        }
        for (number, (name, params, ret)) in exports.iter().enumerate() {
            export_text(name, params, ret, number, &prog.plain, &mut types, &mut head, &mut after);
        }
        let mut text = head;
        text.push_str(&before);
        for file in &files {
            text.push_str(&include_line(file));
        }
        text.push_str(&after);
        let span = match used.first() {
            Some(name) => foreign_of(prog, name).span,
            None => prog.funcs.iter().find(|func| func.export).map_or(crate::diag::Span::new(0, 0), |func| func.name_span),
        };
        let failed = |why: String| {
            if let Some(missing) = crate::link::missing(prog, &blocks) {
                return missing;
            }
            let linked: Vec<String> = libs.iter().map(|lib| format!("`-l{lib}`")).collect();
            let diag = Diag::error(span, "the functions of C this program uses could not be built and linked");
            let diag = match linked.is_empty() {
                true => diag,
                false => diag.note(format!("is every library installed where the C compiler looks? {}", linked.join(", "))),
            };
            diag.note(why)
        };
        #[cfg(windows)]
        crate::sys::remove_old_libraries();
        let dir = private_dir().map_err(|error| failed(format!("no folder for the library could be made: {error}")))?;
        let built = build(&dir, &text, &libs, &prog.lib_dirs, !files.is_empty());
        // The program's own folder, then those of `-L`: where Windows looks for the DLLs the
        // library needs. Linux and macOS find them where the library was linked to look.
        let folders: Vec<PathBuf> = prog.home.iter().chain(&prog.lib_dirs).cloned().collect();
        let opened = built.and_then(|library| crate::sys::open_library(&library, &folders));
        // A DLL that is loaded cannot be deleted: on Windows its folder goes when an interpreter
        // starts after this one has ended (`remove_old_libraries`).
        if !cfg!(windows) {
            let _ = std::fs::remove_dir_all(&dir);
        }
        let handle = opened.map_err(failed)?;
        binary_streams();
        let mut library = Library { exports, ..Library::default() };
        let symbol_of = |name: String| -> Result<*mut c_void, Diag> { crate::sys::library_symbol(handle, &name).map_err(failed) };
        for (number, (key, _)) in values.throughs.iter().enumerate() {
            let found = symbol_of(format!("sg_t{number}"))?;
            // SAFETY: each wrapper that calls through an address was written with this signature.
            library.throughs.insert(key.clone(), unsafe { std::mem::transmute::<*mut c_void, Through>(found) });
        }
        for (name, _, _) in &library.exports {
            let found = symbol_of(short_name(name).to_string())?;
            library.addresses.insert(name.clone(), found as usize);
        }
        if !library.exports.is_empty() {
            let found = symbol_of("sg_set_enter".to_string())?;
            // SAFETY: written with this signature; it keeps the entry for the stubs.
            unsafe { std::mem::transmute::<*mut c_void, SetEnter>(found)(crate::interp::enter) };
        }
        for (number, name) in fixed.iter().enumerate().filter(|(_, name)| values.addressed.contains(**name)) {
            let found = symbol_of(format!("sg_f{number}"))?;
            // SAFETY: each address function was written with this signature.
            let address = unsafe { std::mem::transmute::<*mut c_void, Address>(found) };
            // SAFETY: it takes nothing and gives the address of the function.
            library.addresses.insert((*name).clone(), unsafe { address() } as usize);
        }
        let keys = fixed.iter().map(|name| (*name).clone()).chain(calls.keys().cloned());
        for (number, name) in keys.enumerate() {
            let is_var = prog.cvar_types.contains_key(&name);
            let found = symbol_of(format!("sg_{}{number}", if is_var { 'v' } else { 'w' }))?;
            if is_var {
                // SAFETY: each address function was written with this signature.
                let address = unsafe { std::mem::transmute::<*mut c_void, Address>(found) };
                // SAFETY: it takes nothing and gives the address of the variable.
                library.vars.insert(name, unsafe { address() } as usize);
            } else {
                // SAFETY: each wrapper was written with this signature.
                library.calls.insert(name, unsafe { std::mem::transmute::<*mut c_void, Wrapper>(found) });
            }
        }
        // The library stays open until the process ends: a pointer C gave may point into it.
        Ok(library)
    }

    /// The address of the variable of C named `name`.
    pub fn variable(&self, name: &str) -> usize {
        self.vars[name]
    }

    /// The wrapper of the function of C named `name` (by `variadic_key` for one declared with
    /// `...`), to be called with no borrow of the interpreter held: C may call back into it
    /// (see `call_wrapper`).
    pub fn wrapper(&self, name: &str) -> Wrapper {
        self.calls[name]
    }

    /// What calls through a `CFunc` of these C types.
    pub fn through(&self, key: &str) -> Through {
        self.throughs[key]
    }

    /// The address of the function named `name` as a `CFunc`.
    pub fn address(&self, name: &str) -> usize {
        self.addresses[name]
    }

    /// The `export` function numbered `number`, with what it takes and gives.
    pub fn export(&self, number: usize) -> &(String, Vec<Ty>, Ty) {
        &self.exports[number]
    }
}

/// The full names of the functions of C the program calls, in a fixed order.
pub fn called(prog: &Program, tests: bool) -> Vec<String> {
    used(prog, tests).into_iter().filter(|name| !prog.cvar_types.contains_key(name)).collect()
}

/// The full names of the functions of C the program calls and of the variables of C it uses,
/// in a fixed order.
pub fn used(prog: &Program, tests: bool) -> Vec<String> {
    let mut found: BTreeSet<String> = BTreeSet::new();
    let mut each = |expr: &crate::ast::Expr| match &expr.kind {
        ExprKind::Call { name, callee: Callee::Extern | Callee::Variadic(_), .. } | ExprKind::CVar(name) => {
            found.insert(name.clone());
        }
        // The address of a function of C, as a `CFunc`.
        ExprKind::FuncRef(name) if matches!(expr.ty, Ty::CFunc(..)) && prog.foreign.contains_key(name) => {
            found.insert(name.clone());
        }
        _ => {}
    };
    each_expr(prog, tests, &mut each);
    found.into_iter().collect()
}

/// Each call of a function of C declared with `...`, by `variadic_key`: its name and the
/// types it gives, those of the parameters and then those of the arguments after them.
pub fn variadic_calls(prog: &Program, tests: bool) -> std::collections::BTreeMap<String, (String, Vec<Ty>)> {
    let mut found = std::collections::BTreeMap::new();
    let mut each = |expr: &crate::ast::Expr| {
        if let ExprKind::Call { name, callee: Callee::Variadic(params), .. } = &expr.kind {
            let ret = &prog.foreign[name].1;
            found.insert(variadic_key(name, params, ret), (name.clone(), params.clone()));
        }
    };
    each_expr(prog, tests, &mut each);
    found
}

/// What names the adapter compiled code calls one function of C declared with `...` through:
/// its name and the types of SG the call gives it. Two lists of one C type are two adapters
/// there (a `read CString` hands over its text, a `raw void` itself).
pub fn adapter_key(name: &str, params: &[Ty]) -> String {
    let types: Vec<String> = params.iter().map(Ty::to_string).collect();
    format!("{name}#{}", types.join(","))
}

/// The calls of functions declared with `...`, by `adapter_key`.
pub fn adapter_calls(prog: &Program, tests: bool) -> std::collections::BTreeMap<String, (String, Vec<Ty>)> {
    let mut found = std::collections::BTreeMap::new();
    let mut each = |expr: &crate::ast::Expr| {
        if let ExprKind::Call { name, callee: Callee::Variadic(params), .. } = &expr.kind {
            found.insert(adapter_key(name, params), (name.clone(), params.clone()));
        }
    };
    each_expr(prog, tests, &mut each);
    found
}

/// What a program does with pointers to functions of C: the functions of C whose names it
/// takes as `CFunc` values, the types of its calls through a `CFunc` (by `through_key`), and
/// whether it takes the name of an `export` function as one.
pub struct CValues {
    pub addressed: BTreeSet<String>,
    pub throughs: std::collections::BTreeMap<String, (Vec<Ty>, Ty)>,
    pub exports: bool,
}

pub fn c_values(prog: &Program, tests: bool) -> CValues {
    let mut found = CValues { addressed: BTreeSet::new(), throughs: Default::default(), exports: false };
    let mut each = |expr: &crate::ast::Expr| match &expr.kind {
        ExprKind::FuncRef(name) if matches!(expr.ty, Ty::CFunc(..)) => match prog.foreign.contains_key(name) {
            true => {
                found.addressed.insert(name.clone());
            }
            false => found.exports = true,
        },
        ExprKind::Call { callee: Callee::CValue(params), .. } | ExprKind::Method { callee: Callee::CFieldValue(_, params), .. } => {
            found.throughs.insert(through_key(params, &expr.ty), (params.clone(), expr.ty.clone()));
        }
        _ => {}
    };
    each_expr(prog, tests, &mut each);
    found
}

/// Shows `each` every expression in the bodies of the program, and of its tests with `tests`;
/// of an `if` on the operating system only the branch of this one.
fn each_expr<'p>(prog: &'p Program, tests: bool, each: &mut dyn FnMut(&'p crate::ast::Expr)) {
    let methods = prog.structs.iter().flat_map(|decl| &decl.methods).chain(prog.enums.iter().flat_map(|decl| &decl.methods));
    for func in prog.funcs.iter().chain(methods) {
        for stmt in &func.body {
            crate::waits::walk_here(stmt, each);
        }
    }
    if tests {
        for test in &prog.tests {
            for stmt in &test.body {
                crate::waits::walk_here(stmt, each);
            }
        }
    }
}

/// The C type of a value as the wrapper reads it: a pointer is a pointer, whatever it points
/// at, and a struct is the one `struct_text` writes.
fn c_type(ty: &Ty) -> String {
    match ty {
        Ty::Struct(name) => format!("sg_s_{}", c_struct_name(name)),
        Ty::CFunc(..) => format!("sg_ft_{}", func_type_name(ty)),
        Ty::Char => "uint32_t".to_string(),
        Ty::Atomic(inner) => c_type(inner),
        other => scalar_type(other).to_string(),
    }
}

/// A name for the type of a function of C, made of its types: `CFunc<raw void, int>` is
/// `p_i32`, the return type first.
fn func_type_name(ty: &Ty) -> String {
    match ty {
        Ty::CFunc(params, ret) => {
            let mut parts = vec![func_type_name(ret)];
            parts.extend(params.iter().map(func_type_name));
            format!("F{}E", parts.join("_"))
        }
        Ty::Int(int) => int.name().to_string(),
        Ty::Float(float) => float.name().to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::Void => "void".to_string(),
        Ty::Struct(name) => format!("S{}", c_struct_name(name)),
        _ => "p".to_string(),
    }
}

/// Defines what the C type of `ty` needs, before it is used: a struct, or the type of a
/// function of C, after those they hold.
fn type_text(ty: &Ty, plain: &crate::raw::PlainTypes, done: &mut BTreeSet<String>, text: &mut String) {
    match ty {
        Ty::CFunc(params, ret) => {
            let name = c_type(ty);
            if done.contains(&name) {
                return;
            }
            for inner in params.iter().chain([&**ret]) {
                type_text(inner, plain, done, text);
            }
            done.insert(name.clone());
            let types: Vec<String> = params.iter().map(c_type).collect();
            let list = if types.is_empty() { "void".to_string() } else { types.join(", ") };
            text.push_str(&format!("typedef {} (*{name})({list});\n", c_type(ret)));
        }
        Ty::Array(item, _) => type_text(item, plain, done, text),
        _ => struct_text(ty, plain, done, text),
    }
}

/// A full name as a C name: `shop.Point` is `shop_Point`.
fn c_struct_name(name: &str) -> String {
    name.replace('.', "_")
}

/// Defines the struct `ty` is, if it is one, after those it holds, as C lays out a struct of
/// plain data: the fields in their order, each of its C type.
fn struct_text(ty: &Ty, plain: &crate::raw::PlainTypes, done: &mut BTreeSet<String>, text: &mut String) {
    let item = match ty {
        Ty::Array(item, _) => item,
        other => other,
    };
    let Ty::Struct(name) = item else { return };
    if !done.insert(name.clone()) {
        return;
    }
    let decl = &plain[name];
    for field in &decl.fields {
        type_text(field, plain, done, text);
    }
    text.push_str(&crate::raw::c_definition(&format!("sg_s_{}", c_struct_name(name)), decl, &|ty: &Ty| c_type(ty), &|at| format!("m{at}")));
}

fn scalar_type(ty: &Ty) -> &'static str {
    match ty {
        Ty::Int(int) => match int {
            IntTy::Sbyte => "int8_t",
            IntTy::Byte => "uint8_t",
            IntTy::Short => "int16_t",
            IntTy::Ushort => "uint16_t",
            IntTy::Int => "int32_t",
            IntTy::Uint => "uint32_t",
            IntTy::Long => "int64_t",
            IntTy::Ulong => "uint64_t",
            // As compiled code has them, so that a C file of the program sees the same types.
            IntTy::Nint => "int64_t",
            IntTy::Nuint => "uint64_t",
            IntTy::CLong => "long",
            IntTy::CULong => "unsigned long",
        },
        Ty::Float(FloatTy::Float) => "float",
        Ty::Float(FloatTy::Double) => "double",
        Ty::Bool => "_Bool",
        Ty::Void => "void",
        _ => "void*",
    }
}

pub(crate) use crate::sys::private_dir;

/// Writes the wrappers into `dir` and builds them into a shared library there, in the C of
/// compiled code (C11), with the C compiler compiled code is built with. On Windows the library
/// is a DLL linked with C's library as a DLL (`/MD`), the one the interpreter itself uses, so
/// that memory C gives goes back with `Memory.free` and the other way round, and `errno` is one.
fn build(dir: &Path, text: &str, libs: &BTreeSet<String>, lib_dirs: &[PathBuf], c_files: bool) -> Result<PathBuf, String> {
    let source = dir.join("wrappers.c");
    let library = dir.join(if cfg!(windows) { "wrappers.dll" } else { "wrappers.so" });
    std::fs::write(&source, text).map_err(|error| format!("the wrappers could not be written: {error}"))?;
    let compiler = crate::cc::compiler()?;
    let shown = crate::cc::shown(&compiler);
    let mut cc = std::process::Command::new(&compiler);
    // No warnings: compiled code is built without them too.
    cc.args(["-std=c11", "-shared", "-fno-builtin", "-w"]);
    match cfg!(windows) {
        true => cc.args(["-fuse-ld=lld", "-fms-runtime-lib=dll"]),
        false => cc.arg("-fPIC"),
    };
    cc.arg("-o").arg(&library).arg(&source);
    // A function missing from the libraries is an error here on Linux too, as it is on macOS
    // and Windows.
    #[cfg(target_os = "linux")]
    cc.arg("-Wl,--no-undefined");
    // The folders of `-L`, looked in when the wrappers are linked and when they are opened.
    cc.args(crate::link::folder_flags(lib_dirs, true));
    for lib in libs {
        cc.arg(format!("-l{lib}"));
    }
    let output = cc.output().map_err(|error| format!("the C compiler `{shown}` could not be run: {error}"))?;
    if !output.status.success() {
        if !c_files {
            return Err(format!("the C compiler `{shown}` could not build the wrappers with them: a library or a function is missing"));
        }
        // What the C compiler said about a C file of the program is the program's to read.
        let said = String::from_utf8_lossy(&output.stderr);
        let lines: Vec<&str> = said.lines().filter(|line| !line.contains("wrappers.c")).take(12).collect();
        let mut why = format!("the C compiler `{shown}` could not build the wrappers with them: a library or a function is missing, or a C file has a mistake");
        if !lines.is_empty() {
            why.push_str(&format!("; `{shown}` says:\n{}", lines.join("\n")));
        }
        return Err(why);
    }
    Ok(library)
}

