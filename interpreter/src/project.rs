//! The file `sg.project` beside a program: the packages it uses (choices 1552, 1555-1557).
//!
//! ```text
//! package github.com/ada/shop
//! require xml  github.com/ada/xml     1.4.0
//! require zlib github.com/ada/sg-zlib 2.0.1 unsafe
//! local   xml  ../xml
//! ```
//!
//! One line says one thing, and `//` starts a comment. `package` is the address the project is
//! fetched from when it is a package itself. `require` names a package: the name the program
//! writes after `use` (the last part of the address when it is left out), the address and the
//! version, whose tag is `v1.4.0`. `local` takes a package from a folder instead of fetching it,
//! and only the program's own file has it: the `local` lines of a package are not read.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The name of the file.
pub const FILE: &str = "sg.project";
/// The most an `sg.project` may have (choice 1591): a package's is read before it is trusted.
pub const MAX_BYTES: u64 = 1 << 20;

pub struct Project {
    /// Where the file is.
    pub path: PathBuf,
    pub package: Option<String>,
    pub requires: Vec<Require>,
    pub locals: Vec<Local>,
}

pub struct Require {
    pub name: String,
    pub address: String,
    pub version: Version,
    /// The program allows the package `unsafe` code (choice 1562).
    pub allows_unsafe: bool,
    pub line: usize,
}

pub struct Local {
    pub name: String,
    /// As written; relative to the folder of the file.
    pub folder: PathBuf,
    pub line: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// The `sg.project` in `folder`, if there is one.
pub fn read(folder: &Path) -> Result<Option<Project>, String> {
    let path = folder.join(FILE);
    let file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let text = String::from_utf8(bytes).map_err(|_| format!("cannot read {}: it is not UTF-8", path.display()))?;
    parse(&text, &path).map(Some)
}

/// A name a package can be given: one a folder of modules can have (letters, digits and `_`,
/// not starting with a digit), other than `std` and the names of the language's types
/// (choice 1591).
pub fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    name != crate::source::STD_MODULE
        && !crate::check::is_language_type(name)
        && chars.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A character a terminal does not show as itself: a control character, one that changes the
/// direction of the text or has no width, or a space other than the space (choice 1591). Text
/// a package gives is not printed with one in it.
pub fn is_unseen(c: char) -> bool {
    (c.is_control() && c != '\t')
        || (c.is_whitespace() && c != ' ' && c != '\t')
        || matches!(c, '\u{AD}' | '\u{34F}' | '\u{61C}' | '\u{115F}' | '\u{1160}' | '\u{17B4}' | '\u{17B5}' | '\u{180B}'..='\u{180F}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{3164}' | '\u{FE00}'..='\u{FE0F}' | '\u{FEFF}' | '\u{FFA0}' | '\u{FFF0}'..='\u{FFFB}' | '\u{1D173}'..='\u{1D17A}' | '\u{E0000}'..='\u{E0FFF}')
}

/// `text` with each character a terminal does not show as itself written as its number,
/// `\u{202e}`.
pub fn printable(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match is_unseen(c) || c == '\t' {
            true => out.extend(c.escape_unicode()),
            false => out.push(c),
        }
    }
    out
}

pub fn parse(text: &str, path: &Path) -> Result<Project, String> {
    let mut project = Project { path: path.to_path_buf(), package: None, requires: Vec::new(), locals: Vec::new() };
    if text.len() as u64 > MAX_BYTES {
        return Err(format!("{}: it has more than the 1 MB an `{FILE}` may have", path.display()));
    }
    // The lines that gave each name, and each address with its major version.
    let mut names: HashMap<String, usize> = HashMap::new();
    let mut addresses: HashMap<(String, u64), usize> = HashMap::new();
    let mut local_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (at, line) in text.lines().enumerate() {
        let number = at + 1;
        let fail = |message: String| format!("{}:{number}: {message}", path.display());
        if line.chars().any(is_unseen) {
            return Err(fail("the line has a character that is not seen as itself: a control character, a mark of direction or one with no width".into()));
        }
        // `//` starts a comment where a word may start: at the start of the line or after a
        // space. An address has no `//` in it, so one written into it is not taken away.
        let line = match line.match_indices("//").find(|(at, _)| *at == 0 || line[..*at].ends_with([' ', '\t'])) {
            Some((at, _)) => &line[..at],
            None => line,
        };
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some((&word, rest)) = words.split_first() else { continue };
        match word {
            "package" => {
                let [address] = rest else { return Err(fail("`package` is followed by the address of the project: `package github.com/ada/shop`".into())) };
                if project.package.is_some() {
                    return Err(fail("`package` is written twice".into()));
                }
                address_ok(address).map_err(fail)?;
                project.package = Some(address.to_string());
            }
            "require" => {
                let (name, address, version, flag) = match rest {
                    [address, version] if address.contains('/') => (None, *address, *version, None),
                    [address, version, flag] if address.contains('/') => (None, *address, *version, Some(*flag)),
                    [name, address, version] => (Some(*name), *address, *version, None),
                    [name, address, version, flag] => (Some(*name), *address, *version, Some(*flag)),
                    _ => return Err(fail("`require` is followed by a name, the address and the version: `require xml github.com/ada/xml 1.4.0`".into())),
                };
                address_ok(address).map_err(fail)?;
                let version = version_of(version).map_err(fail)?;
                let allows_unsafe = match flag {
                    None => false,
                    Some("unsafe") => true,
                    Some(other) => return Err(fail(format!("`{other}` after the version: only `unsafe` can be written there"))),
                };
                let name = match name {
                    Some(name) => name.to_string(),
                    None => address.rsplit('/').next().unwrap_or(address).to_string(),
                };
                if !is_name(&name) {
                    let message = match name == crate::source::STD_MODULE {
                        true => "`std` is the language's own module, and a package cannot have its name".to_string(),
                        false if crate::check::is_language_type(&name) => format!("`{name}` is a type of the language, and a package cannot have its name"),
                        false => format!("`{name}` cannot be written after `use`: a name is letters, digits and `_`, and it does not start with a digit"),
                    };
                    return Err(fail(format!("{message}; give the package a name in front of its address: `require {} {address} {version}`", suggested(&name))));
                }
                if let Some(other) = names.insert(name.clone(), number) {
                    return Err(fail(format!("`{name}` is the name of the package of line {other} too: give one of them another name")));
                }
                if let Some(other) = addresses.insert((address.to_string(), version.major), number) {
                    return Err(fail(format!("{address} {} is required on line {other} too", version.major)));
                }
                project.requires.push(Require { name, address: address.to_string(), version, allows_unsafe, line: number });
            }
            "local" => {
                let [name, ..] = rest else { return Err(fail("`local` is followed by the name of a package and its folder: `local xml ../xml`".into())) };
                // The folder is the rest of the line, which may have spaces in it.
                let after = line.trim_start().strip_prefix("local").unwrap_or("").trim_start();
                let folder = after.strip_prefix(name).unwrap_or("").trim();
                if folder.is_empty() {
                    return Err(fail(format!("`local {name}` is followed by the folder of the package: `local {name} ../{name}`")));
                }
                if !local_names.insert(name.to_string()) {
                    return Err(fail(format!("`local {name}` is written twice")));
                }
                project.locals.push(Local { name: name.to_string(), folder: PathBuf::from(folder), line: number });
            }
            _ => return Err(fail(format!("`{word}` is not a line of `{FILE}`: a line is `package`, `require` or `local`"))),
        }
    }
    for local in &project.locals {
        if !names.contains_key(&local.name) {
            let fail = format!("{}:{}: `local {}` names no package: a `require` line gives the package its name and address", path.display(), local.line, local.name);
            return Err(fail);
        }
    }
    Ok(project)
}

/// A name to suggest for a package whose address ends in one that cannot be written.
pub fn suggested(name: &str) -> String {
    let mut out: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    if let Some(rest) = out.strip_prefix("sg_") {
        out = rest.to_string();
    }
    if crate::check::is_language_type(&out) {
        out = out.to_ascii_lowercase();
    }
    if out.is_empty() || out == crate::source::STD_MODULE || out.starts_with(|c: char| c.is_ascii_digit()) || crate::check::is_language_type(&out) {
        out = format!("p{out}");
    }
    out
}

/// An address is a host and a path, as Go writes them: `github.com/ada/xml`. It is fetched over
/// HTTPS (choice 1558), so no scheme is written, and nothing in it can lead the fetch elsewhere.
pub fn address_ok(address: &str) -> Result<(), String> {
    let bad = |why: &str| Err(format!("`{address}` is not the address of a package: {why}; it is written as `github.com/ada/xml`"));
    if address.contains("://") || address.starts_with("git@") {
        return bad("the address is written without a scheme or a user");
    }
    let parts: Vec<&str> = address.split('/').collect();
    let host = parts[0];
    if parts.len() < 2 || !host.contains('.') || host.starts_with(['.', '-']) || !host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return bad("it starts with the name of a host, then a path");
    }
    for part in &parts[1..] {
        if part.is_empty() || *part == "." || *part == ".." || part.starts_with(['.', '-']) {
            return bad("each part of the path is a name");
        }
        if !part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '~')) {
            return bad("a part of the path is letters, digits and `. - _ ~`");
        }
    }
    Ok(())
}

/// `1.4.0`: three numbers, without a `v` and without zeros in front, as semantic versioning
/// writes them; the tag of the version is `v1.4.0`.
pub fn version_of(written: &str) -> Result<Version, String> {
    let bad = || format!("`{written}` is not a version: a version is three numbers, `1.4.0`, and its tag is `v1.4.0`");
    let numbers: Vec<&str> = written.split('.').collect();
    let [major, minor, patch] = numbers[..] else { return Err(bad()) };
    let number = |part: &str| match part.parse::<u64>() {
        Ok(value) if part.chars().all(|c| c.is_ascii_digit()) && (part == "0" || !part.starts_with('0')) => Ok(value),
        _ => Err(bad()),
    };
    Ok(Version { major: number(major)?, minor: number(minor)?, patch: number(patch)? })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> Result<Project, String> {
        parse(text, Path::new("sg.project"))
    }

    #[test]
    fn reads_the_lines() {
        let project = parsed("// the shop\npackage github.com/ada/shop\n\nrequire xml  github.com/ada/xml 1.4.0\nrequire zlib github.com/ada/sg-zlib 2.0.1 unsafe // C inside\nrequire github.com/bob/utf 0.3.12\nlocal xml ../my xml\n").unwrap();
        assert_eq!(project.package.as_deref(), Some("github.com/ada/shop"));
        let names: Vec<(&str, &str, String, bool)> = project.requires.iter().map(|r| (r.name.as_str(), r.address.as_str(), r.version.to_string(), r.allows_unsafe)).collect();
        assert_eq!(
            names,
            [("xml", "github.com/ada/xml", "1.4.0".to_string(), false), ("zlib", "github.com/ada/sg-zlib", "2.0.1".to_string(), true), ("utf", "github.com/bob/utf", "0.3.12".to_string(), false)]
        );
        assert_eq!(project.locals[0].folder, PathBuf::from("../my xml"));
    }

    #[test]
    fn refuses_what_is_wrong() {
        let error = |text: &str| parsed(text).err().unwrap_or_default();
        assert!(error("require github.com/ada/sg-zlib 2.0.1").contains("give the package a name in front of its address: `require zlib github.com/ada/sg-zlib 2.0.1`"));
        assert!(error("require xml github.com/ada/xml v1.4.0").contains("its tag is `v1.4.0`"));
        assert!(error("require xml github.com/ada/xml 1.04.0").contains("not a version"));
        assert!(error("require xml https://github.com/ada/xml 1.4.0").contains("without a scheme"));
        assert!(error("require xml github.com/../xml 1.4.0").contains("each part of the path is a name"));
        assert!(error("require xml localhost/xml 1.4.0").contains("name of a host"));
        assert!(error("require xml github.com/ada/xml 1.4.0 safe").contains("only `unsafe`"));
        assert!(error("require std github.com/ada/std 1.0.0").contains("language's own module"));
        assert!(error("require xml github.com/ada/xml 1.4.0\nrequire xml github.com/bob/xml 1.0.0").contains("line 1 too"));
        assert!(error("require a github.com/ada/xml 1.4.0\nrequire b github.com/ada/xml 1.5.0").contains("required on line 1 too"));
        assert!(parsed("require a github.com/ada/xml 1.4.0\nrequire b github.com/ada/xml 2.0.0").is_ok());
        assert!(error("local xml ../xml").contains("names no package"));
        assert!(error("requires xml github.com/ada/xml 1.4.0").contains("is not a line of `sg.project`"));
        assert!(error("package github.com/a/b\npackage github.com/a/c").contains("twice"));
        // A name of a type of the language is a type's, and the name suggested is not one.
        assert!(error("require github.com/ada/Hex 1.0.0").contains("`Hex` is a type of the language, and a package cannot have its name; give the package a name in front of its address: `require hex github.com/ada/Hex 1.0.0`"));
        assert!(error("require string github.com/ada/x 1.0.0").contains("`require pstring github.com/ada/x 1.0.0`"));
        // A line with a character that is not seen as itself is not printed back.
        for line in ["require xml github.com/ada/xml 1.4.0 \u{202e}", "requir\u{1b}[2Ke xml", "local xml ../x\u{200b}ml"] {
            let said = error(line);
            assert!(said.contains("a character that is not seen as itself") && !said.contains(line), "{said}");
        }
        assert!(error(&"// x\n".repeat(300_000)).contains("more than the 1 MB"));
    }
}
