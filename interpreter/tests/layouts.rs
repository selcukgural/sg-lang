//! The layout of structs and unions of plain data, as the interpreter computes it (`raw.rs`),
//! against what compilers of C give: the C compiler of this machine for its own layout, run as a
//! program, and `clang`, when it is there, for every system the language runs on, from the
//! layouts it dumps. Compiled code takes its layout from C; the interpreter must lay out the same
//! bytes, or a struct handed between the two would be read wrong.

use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;
use std::process::Command;

use sg::raw::{self, Abi, Place, Plain, PlainTypes};
use sg::types::{FloatTy, IntTy, Ty};

fn int(ty: IntTy) -> Ty {
    Ty::Int(ty)
}

fn named(name: &str) -> Ty {
    Ty::Struct(name.to_string())
}

/// A struct: each field with its width when it is a bit field, and the bits left empty.
fn plain(fields: &[(Ty, Option<u32>)], gaps: &[(usize, Ty, u32)]) -> Plain {
    Plain {
        fields: fields.iter().map(|(ty, _)| ty.clone()).collect(),
        bits: fields.iter().map(|(_, bits)| *bits).collect(),
        gaps: gaps.to_vec(),
        ..Plain::default()
    }
}

/// The structs looked at, in an order where each comes after those it holds.
fn cases() -> Vec<(&'static str, Plain)> {
    use IntTy::*;
    let b = |ty: IntTy, width: u32| (int(ty), Some(width));
    let f = |ty: Ty| (ty, None);
    vec![
        ("A", plain(&[b(Uint, 3), b(Byte, 4), b(Ushort, 9), f(int(Byte)), b(Long, 5), f(int(Byte))], &[(3, int(Int), 0), (5, int(Uint), 3)])),
        ("B", plain(&[f(int(Byte)), f(int(Byte))], &[(1, int(Int), 0)])),
        ("C", plain(&[f(int(Byte)), f(int(Byte))], &[(1, int(Uint), 5)])),
        ("Flags", plain(&[b(Uint, 4), b(Uint, 4), (Ty::Bool, Some(1)), f(int(Ushort))], &[(3, int(Uint), 7)])),
        ("Wide", plain(&[b(Ulong, 60), b(Ulong, 10), b(Uint, 30), b(Uint, 3)], &[])),
        ("Mixed", plain(&[b(Byte, 7), b(Ushort, 9), b(Byte, 2), f(int(Int)), b(Short, 15), b(Short, 2)], &[])),
        ("Signed", plain(&[b(Int, 3), b(Int, 29)], &[])),
        ("Tail", plain(&[b(Int, 5)], &[(1, int(Long), 0)])),
        ("Front", plain(&[b(Byte, 2), f(int(Ushort))], &[(0, int(Ushort), 3)])),
        ("Truths", plain(&[(Ty::Bool, Some(1)), (Ty::Bool, Some(1)), f(int(Byte)), (Ty::Bool, Some(1))], &[])),
        ("Scalars", plain(&[f(Ty::Char), f(Ty::Float(FloatTy::Float)), f(Ty::Float(FloatTy::Double)), f(int(Byte)), f(Ty::Array(Box::new(int(Ushort)), 3))], &[])),
        ("Bits", Plain { union: true, ..plain(&[f(int(Uint)), f(Ty::Float(FloatTy::Float)), f(Ty::Array(Box::new(int(Byte)), 3)), f(named("Flags"))], &[]) }),
        ("Header", Plain { packed: true, ..plain(&[f(int(Byte)), f(int(Uint)), f(Ty::Float(FloatTy::Double)), f(int(Ushort)), f(named("Scalars"))], &[]) }),
        ("Narrow", Plain { union: true, packed: true, ..plain(&[f(int(Uint)), f(int(Byte))], &[]) }),
        ("Slot", Plain { align: 64, ..plain(&[f(int(Long)), f(Ty::Array(Box::new(int(Byte)), 3))], &[]) }),
        ("Holder", plain(&[f(int(Byte)), f(named("Slot"))], &[])),
        ("Nested", plain(&[f(int(Byte)), f(named("Bits")), f(named("Header")), b(Uint, 3)], &[])),
        ("Wider", Plain { align: 32, union: true, ..plain(&[f(int(Short)), f(named("Signed"))], &[]) }),
        // C's `long`: 4 bytes on Windows, 8 elsewhere.
        ("Longs", plain(&[f(int(Byte)), f(int(CLong)), f(int(Ushort)), f(int(CULong)), f(int(Byte))], &[])),
    ]
}

/// The C type of a field, made of what every compiler of C defines without a header.
fn c_type(ty: &Ty) -> String {
    match ty {
        Ty::Int(int) => {
            let name = match int {
                IntTy::Sbyte => "__INT8_TYPE__",
                IntTy::Byte => "__UINT8_TYPE__",
                IntTy::Short => "__INT16_TYPE__",
                IntTy::Ushort => "__UINT16_TYPE__",
                IntTy::Int => "__INT32_TYPE__",
                IntTy::Uint => "__UINT32_TYPE__",
                IntTy::Long | IntTy::Nint => "__INT64_TYPE__",
                IntTy::Ulong | IntTy::Nuint => "__UINT64_TYPE__",
                IntTy::CLong => "long",
                IntTy::CULong => "unsigned long",
            };
            name.to_string()
        }
        Ty::Bool => "_Bool".to_string(),
        Ty::Char => "__UINT32_TYPE__".to_string(),
        Ty::Float(FloatTy::Float) => "float".to_string(),
        Ty::Float(FloatTy::Double) => "double".to_string(),
        Ty::Struct(name) => name.clone(),
        other => panic!("no C type for `{other}` here"),
    }
}

/// The C definitions of every case, as `raw::c_definition` writes them.
fn definitions(cases: &[(&str, Plain)]) -> String {
    let mut text = String::from(raw::C_ALIGNED);
    for (name, decl) in cases {
        text.push_str(&raw::c_definition(name, decl, &c_type, &|at| format!("f{at}")));
    }
    text
}

fn types(cases: &[(&str, Plain)]) -> PlainTypes {
    cases.iter().map(|(name, decl)| (name.to_string(), decl.clone())).collect()
}

/// For each case: its size and alignment in bytes, and where each field starts and how many
/// bits it has (a field that is not a bit field: its byte times 8, and no width).
type Found = HashMap<String, (usize, usize, Vec<(usize, Option<u32>)>)>;

fn computed(cases: &[(&str, Plain)], abi: Abi) -> Found {
    let all = types(cases);
    cases
        .iter()
        .map(|(name, decl)| {
            let shape = raw::shape_for(name, &all, abi);
            let places = shape
                .places
                .iter()
                .zip(&decl.bits)
                .map(|(place, bits)| match place {
                    Place::Bytes(at) => (at * 8, None),
                    Place::Bits { at, width } => (*at, bits.is_some().then_some(*width)),
                })
                .collect();
            (name.to_string(), (shape.size, shape.align, places))
        })
        .collect()
}

/// The layout of this machine, from a C program that prints it: the start of a bit field is
/// the lowest bit that writing all ones into it sets.
fn of_this_machine(cases: &[(&str, Plain)], work: &Path) -> Found {
    let mut program = String::from("#include <stdio.h>\n#include <stddef.h>\n#include <string.h>\n");
    program.push_str(&definitions(cases));
    program.push_str("static void bits(const unsigned char* p, size_t n, const char* name, int field) {\n    size_t first = (size_t)-1, count = 0;\n    for (size_t i = 0; i < n * 8; i++) if (p[i / 8] >> (i % 8) & 1) { if (first == (size_t)-1) first = i; count++; }\n    printf(\"%s %d %zu %zu\\n\", name, field, first, count);\n}\nint main(void) {\n");
    for (name, decl) in cases {
        let _ = writeln!(program, "    printf(\"{name} size %zu %zu\\n\", sizeof({name}), _Alignof({name}));");
        for (at, bits) in decl.bits.iter().enumerate() {
            match bits {
                Some(_) => {
                    let _ = writeln!(program, "    {{ {name} v; memset(&v, 0, sizeof v); v.f{at} = -1; bits((const unsigned char*)&v, sizeof v, \"{name}\", {at}); }}");
                }
                None => {
                    let _ = writeln!(program, "    printf(\"{name} {at} %zu -\\n\", offsetof({name}, f{at}) * 8);");
                }
            }
        }
    }
    program.push_str("    return 0;\n}\n");
    let source = work.join("layouts.c");
    std::fs::write(&source, program).unwrap();
    let binary = work.join("layouts");
    let built = Command::new("cc").arg("-w").arg("-o").arg(&binary).arg(&source).output().expect("a C compiler (`cc`) is needed to test the layout of C");
    assert!(built.status.success(), "cc did not build the layouts:\n{}", String::from_utf8_lossy(&built.stderr));
    let out = Command::new(&binary).output().unwrap();
    let mut found: Found = HashMap::new();
    for line in String::from_utf8(out.stdout).unwrap().lines() {
        let parts: Vec<&str> = line.split(' ').collect();
        let entry = found.entry(parts[0].to_string()).or_insert((0, 0, Vec::new()));
        match parts[1] {
            "size" => (entry.0, entry.1) = (parts[2].parse().unwrap(), parts[3].parse().unwrap()),
            _ => entry.2.push((parts[2].parse().unwrap(), parts[3].parse().ok())),
        }
    }
    found
}

/// The layout `clang` gives for `target`, from the layouts it dumps; the fields as the cases
/// name them, without the bits left empty.
fn of_target(cases: &[(&str, Plain)], target: &str, work: &Path) -> Found {
    let source = work.join(format!("dump-{target}.c"));
    std::fs::write(&source, format!("{}int sg_sizes = 0{};\n", definitions(cases), cases.iter().map(|(name, _)| format!(" + sizeof({name})")).collect::<String>())).unwrap();
    let out = Command::new("clang").args(["-target", target, "-fsyntax-only", "-Xclang", "-fdump-record-layouts-simple"]).arg(&source).output().unwrap();
    assert!(out.status.success(), "clang did not read the layouts for {target}:\n{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).unwrap();
    let mut found: Found = HashMap::new();
    let mut current = String::new();
    let (mut size, mut align) = (0, 0);
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("Type: ") {
            current = rest.rsplit(' ').next().unwrap().to_string();
        } else if let Some(bits) = line.strip_prefix("Size:") {
            size = bits.parse::<usize>().unwrap() / 8;
        } else if let Some(bits) = line.strip_prefix("Alignment:") {
            align = bits.parse::<usize>().unwrap() / 8;
        } else if let Some(list) = line.strip_prefix("FieldOffsets: [") {
            let offsets: Vec<usize> = list.trim_end_matches("]>").split(", ").filter(|item| !item.is_empty()).map(|item| item.parse().unwrap()).collect();
            let decl = &cases.iter().find(|(name, _)| *name == current).unwrap().1;
            // The members in the order C sees them: the bits left empty before each field.
            let mut fields = Vec::new();
            let mut at = 0;
            for (index, bits) in decl.bits.iter().enumerate() {
                at += decl.gaps.iter().filter(|gap| gap.0 == index).count();
                fields.push((offsets[at], *bits));
                at += 1;
            }
            found.insert(current.clone(), (size, align, fields));
        }
    }
    found
}

fn compare(what: &str, computed: &Found, found: &Found) -> Vec<String> {
    let mut wrong = Vec::new();
    for (name, (size, align, places)) in computed {
        let Some((c_size, c_align, c_places)) = found.get(name) else {
            wrong.push(format!("{what}: `{name}` was not found"));
            continue;
        };
        if (size, align) != (c_size, c_align) {
            wrong.push(format!("{what}: `{name}` has size {size} and alignment {align} here, {c_size} and {c_align} in C"));
        }
        for (field, ((at, bits), (c_at, c_bits))) in places.iter().zip(c_places).enumerate() {
            if at != c_at || (bits.is_some() && bits != c_bits) {
                wrong.push(format!("{what}: field {field} of `{name}` starts at bit {at} here, at bit {c_at} in C"));
            }
        }
    }
    wrong.sort();
    wrong
}

#[test]
fn layouts_of_this_machine() {
    let work = Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("layouts");
    std::fs::create_dir_all(&work).unwrap();
    let cases = cases();
    let wrong = compare("this machine", &computed(&cases, raw::HOST), &of_this_machine(&cases, &work));
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Every system the language runs on, with `clang`: macOS, Linux on x86-64 and on 64-bit Arm,
/// RISC-V, and Windows with each of its compilers.
#[test]
fn layouts_of_every_system() {
    if Command::new("clang").arg("--version").output().is_err() {
        // Linux in its container has only gcc: `layouts_of_this_machine` tests its own layout.
        eprintln!("no clang: the layouts of other systems are tested where it is");
        return;
    }
    let work = Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("layouts");
    std::fs::create_dir_all(&work).unwrap();
    let cases = cases();
    let targets = [
        ("arm64-apple-darwin", Abi::Unix),
        ("x86_64-apple-darwin", Abi::Unix),
        ("x86_64-linux-gnu", Abi::Unix),
        ("aarch64-linux-gnu", Abi::Arm),
        ("riscv64-linux-gnu", Abi::Unix),
        ("x86_64-pc-windows-msvc", Abi::Microsoft),
        ("aarch64-pc-windows-msvc", Abi::Microsoft),
        ("x86_64-w64-mingw32", Abi::Microsoft),
    ];
    let mut wrong = Vec::new();
    for (target, abi) in targets {
        wrong.extend(compare(target, &computed(&cases, abi), &of_target(&cases, target, &work)));
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
