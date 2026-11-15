//! Functions of C in the interpreter.
//!
//! The interpreter cannot call a C function whose signature it learns only when the program
//! is read, and the language takes no dependency (such as libffi) to do it. So, before the
//! program runs, it writes a small C file with a wrapper for each function of C the program
//! calls, all with one signature, `void w(void **args, void *ret)`, builds it as a shared
//! library with `cc`, linked with the libraries the `extern` blocks name, and opens it with
//! `dlopen`. A call writes each argument into a slot of its own, as the C type it is, and hands
//! the wrapper the addresses of the slots.
//!
//! The library is built in a folder of its own that only this user can open, made new for
//! each run and removed once the library is open: a library left where another user could
//! write would run that user's code. Nothing is kept between runs.

use std::collections::{BTreeSet, HashMap};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};

use crate::ast::{short_name, Callee, ExprKind, Func, Program};
use crate::diag::Diag;
use crate::types::{FloatTy, IntTy, Ty};

extern "C" {
    fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    fn dlerror() -> *const c_char;
}

/// `dlopen`: every symbol is found when the library is opened, so a missing one is an error
/// there and not in the middle of the run. The same value on Linux and macOS.
const RTLD_NOW: c_int = 2;

/// What a wrapper is: it reads each argument from the slot whose address it is given, calls
/// the function, and writes what it gives into `ret`.
type Wrapper = unsafe extern "C" fn(*mut *mut c_void, *mut c_void);

/// The wrappers of the functions of C a program calls, by their full name in the program.
#[derive(Default)]
pub struct Library {
    calls: HashMap<String, Wrapper>,
}

/// One value as C takes it: eight bytes, enough for any argument, and aligned for each.
pub type Slot = u64;

impl Library {
    /// Builds and opens the wrappers of the functions of C that the program calls, in the
    /// bodies of its functions and methods (and of its tests, with `tests`). A program that
    /// calls none gets an empty library and nothing is built.
    pub fn open(prog: &Program, tests: bool) -> Result<Library, Diag> {
        let called = called(prog, tests);
        if called.is_empty() {
            return Ok(Library::default());
        }
        let funcs: HashMap<&str, &Func> = prog.funcs.iter().map(|func| (func.name.as_str(), func)).collect();
        let mut text = String::from("#include <stdint.h>\n");
        let mut libs: BTreeSet<&str> = BTreeSet::new();
        // The structs C is given or gives by value, each defined once.
        let mut structs: BTreeSet<String> = BTreeSet::new();
        // A C function declared in two modules is one function of C.
        let mut declared: BTreeSet<&str> = BTreeSet::new();
        for (number, name) in called.iter().enumerate() {
            let func = funcs[name.as_str()];
            if let Some(lib) = &func.foreign.as_ref().expect("a function of C").lib {
                libs.insert(lib);
            }
            let (params, ret) = &prog.foreign[name.as_str()];
            let c_name = short_name(name);
            for ty in params.iter().chain([ret]) {
                struct_text(ty, &prog.plain, &mut structs, &mut text);
            }
            let types: Vec<String> = params.iter().map(c_type).collect();
            if declared.insert(c_name) {
                let list = if types.is_empty() { "void".to_string() } else { types.join(", ") };
                text.push_str(&format!("{} {c_name}({list});\n", c_type(ret)));
            }
            // A struct given by value lies elsewhere, and its slot holds its address; so does
            // the room for one given back, in the slot after the arguments.
            let args: Vec<String> = types
                .iter()
                .zip(params)
                .enumerate()
                .map(|(at, (ty, param))| match param {
                    Ty::Struct(_) => format!("**({ty}**)a[{at}]"),
                    _ => format!("*({ty}*)a[{at}]"),
                })
                .collect();
            let call = format!("{c_name}({})", args.join(", "));
            let body = match ret {
                Ty::Void => format!("(void)r; {call};"),
                Ty::Struct(_) => format!("(void)r; **({}**)a[{}] = {call};", c_type(ret), params.len()),
                _ => format!("*({}*)r = {call};", c_type(ret)),
            };
            text.push_str(&format!("void sg_w{number}(void **a, void *r) {{ (void)a; {body} }}\n"));
        }
        let span = funcs[called[0].as_str()].foreign.as_ref().expect("a function of C").span;
        let failed = |why: String| {
            let linked: Vec<String> = libs.iter().map(|lib| format!("`-l{lib}`")).collect();
            let diag = Diag::error(span, "the functions of C this program calls could not be built and linked");
            let diag = match linked.is_empty() {
                true => diag,
                false => diag.note(format!("is every library installed where the C compiler looks? {}", linked.join(", "))),
            };
            diag.note(why)
        };
        let dir = private_dir().map_err(|error| failed(format!("no folder for the library could be made: {error}")))?;
        let built = build(&dir, &text, &libs);
        let opened = built.and_then(|library| {
            let path = CString::new(library.as_os_str().as_encoded_bytes()).map_err(|_| "the path of the library holds a zero byte".to_string())?;
            // SAFETY: `path` ends in a zero byte; the library was built here from the text above.
            let handle = unsafe { dlopen(path.as_ptr(), RTLD_NOW) };
            if handle.is_null() {
                return Err(format!("`dlopen` says: {}", last_error()));
            }
            Ok(handle)
        });
        let _ = std::fs::remove_dir_all(&dir);
        let handle = opened.map_err(failed)?;
        let mut calls = HashMap::new();
        for (number, name) in called.iter().enumerate() {
            let symbol = CString::new(format!("sg_w{number}")).expect("no zero byte");
            // SAFETY: `handle` is open and `symbol` ends in a zero byte.
            let found = unsafe { dlsym(handle, symbol.as_ptr()) };
            if found.is_null() {
                return Err(failed(format!("`dlsym` says: {}", last_error())));
            }
            // SAFETY: each wrapper was written with this signature.
            calls.insert(name.clone(), unsafe { std::mem::transmute::<*mut c_void, Wrapper>(found) });
        }
        // The library stays open until the process ends: a pointer C gave may point into it.
        Ok(Library { calls })
    }

    /// Calls the function of C named `name` with arguments already in their slots, and gives
    /// the slot of what it returned.
    pub fn call(&self, name: &str, slots: &mut [Slot]) -> Slot {
        let wrapper = self.calls[name];
        let mut addresses: Vec<*mut c_void> = slots.iter_mut().map(|slot| (slot as *mut Slot).cast::<c_void>()).collect();
        let mut ret: Slot = 0;
        // SAFETY: the wrapper reads as many slots as the function takes, each as the C type it
        // was declared with, and writes at most eight bytes into `ret`. What the function of C
        // does is the program's to answer for: it is called inside `unsafe`.
        unsafe { wrapper(addresses.as_mut_ptr(), (&mut ret as *mut Slot).cast::<c_void>()) };
        ret
    }
}

/// The full names of the functions of C the program calls, in a fixed order.
pub fn called(prog: &Program, tests: bool) -> Vec<String> {
    let mut found: BTreeSet<String> = BTreeSet::new();
    let mut each = |expr: &crate::ast::Expr| {
        if let ExprKind::Call { name, callee: Callee::Extern, .. } = &expr.kind {
            found.insert(name.clone());
        }
    };
    let methods = prog.structs.iter().flat_map(|decl| &decl.methods).chain(prog.enums.iter().flat_map(|decl| &decl.methods));
    for func in prog.funcs.iter().chain(methods) {
        for stmt in &func.body {
            crate::waits::walk_stmt(stmt, &mut each, &mut |_| {});
        }
    }
    if tests {
        for test in &prog.tests {
            for stmt in &test.body {
                crate::waits::walk_stmt(stmt, &mut each, &mut |_| {});
            }
        }
    }
    found.into_iter().collect()
}

/// The C type of a value as the wrapper reads it: a pointer is a pointer, whatever it points
/// at, and a struct is the one `struct_text` writes.
fn c_type(ty: &Ty) -> String {
    match ty {
        Ty::Struct(name) => format!("struct sg_s_{}", c_struct_name(name)),
        Ty::Char => "uint32_t".to_string(),
        other => scalar_type(other).to_string(),
    }
}

/// A full name as a C name: `shop.Point` is `shop_Point`.
fn c_struct_name(name: &str) -> String {
    name.replace('.', "_")
}

/// Defines the struct `ty` is, if it is one, after those it holds, as C lays out a struct of
/// plain data: the fields in their order, each of its C type.
fn struct_text(ty: &Ty, plain: &HashMap<String, Vec<Ty>>, done: &mut BTreeSet<String>, text: &mut String) {
    let item = match ty {
        Ty::Array(item, _) => item,
        other => other,
    };
    let Ty::Struct(name) = item else { return };
    if !done.insert(name.clone()) {
        return;
    }
    let fields = &plain[name];
    for field in fields {
        struct_text(field, plain, done, text);
    }
    let mut members = String::new();
    for (at, field) in fields.iter().enumerate() {
        match field {
            Ty::Array(item, length) => members.push_str(&format!(" {} m{at}[{length}];", c_type(item))),
            other => members.push_str(&format!(" {} m{at};", c_type(other))),
        }
    }
    text.push_str(&format!("struct sg_s_{} {{{members} }};\n", c_struct_name(name)));
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
            IntTy::Nint => "intptr_t",
            IntTy::Nuint => "uintptr_t",
        },
        Ty::Float(FloatTy::Float) => "float",
        Ty::Float(FloatTy::Double) => "double",
        Ty::Bool => "_Bool",
        Ty::Void => "void",
        _ => "void*",
    }
}

/// A new folder in the temporary folder that only this user can open.
fn private_dir() -> std::io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let base = std::env::temp_dir();
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |time| time.as_nanos());
    for attempt in 0..100u32 {
        let dir = base.join(format!("sg-ffi-{}-{seed:x}-{attempt}", std::process::id()));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => return Ok(dir),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::other("every name tried was taken"))
}

/// Writes the wrappers into `dir` and builds them into a shared library there.
fn build(dir: &Path, text: &str, libs: &BTreeSet<&str>) -> Result<PathBuf, String> {
    let source = dir.join("wrappers.c");
    let library = dir.join("wrappers.so");
    std::fs::write(&source, text).map_err(|error| format!("the wrappers could not be written: {error}"))?;
    let mut cc = std::process::Command::new("cc");
    // No warnings: the declarations of C functions such as `strlen` differ from the C library's
    // in the names of their types, which is the same thing to the machine.
    cc.args(["-shared", "-fPIC", "-fno-builtin", "-w", "-o"]).arg(&library).arg(&source);
    // A function missing from the libraries is an error here on Linux too, as it is on macOS.
    #[cfg(target_os = "linux")]
    cc.arg("-Wl,--no-undefined");
    for lib in libs {
        cc.arg(format!("-l{lib}"));
    }
    let output = cc.output().map_err(|error| format!("the C compiler `cc` could not be run: {error}"))?;
    if !output.status.success() {
        return Err("the C compiler `cc` could not build the wrappers with them: a library or a function is missing".to_string());
    }
    Ok(library)
}

fn last_error() -> String {
    // SAFETY: `dlerror` gives a text that ends in a zero byte, or `null`.
    let text = unsafe { dlerror() };
    if text.is_null() {
        return "nothing".to_string();
    }
    // SAFETY: as above.
    unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned()
}
