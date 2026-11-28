//! `sg help` (choice 2082): the page of a word of SG in the terminal, from `sg` itself, with no network.
//!
//! The pages are the text `man` shows for them, made by `website/plain/build.mjs` from the same Markdown as the
//! site and the manual pages (`website/words/`) and written into `src/help/`, which `build.rs` lists. Change
//! the Markdown and build the site; never the files here.

include!(concat!(env!("OUT_DIR"), "/help.rs"));

/// One name of the index, as a line of `names.tsv` gives it.
struct Name<'a> {
    name: &'a str,
    kind: &'a str,
    part: &'a str,
    summary: &'a str,
    /// The file of its page in `src/help/`, when it has one.
    page: &'a str,
    address: &'a str,
}

fn names() -> impl Iterator<Item = Name<'static>> {
    NAMES.lines().filter(|line| !line.is_empty() && !line.starts_with('#')).map(|line| {
        let mut fields = line.split('\t');
        let mut next = || fields.next().unwrap_or("");
        Name { name: next(), kind: next(), part: next(), summary: next(), page: next(), address: next() }
    })
}

fn page(file: &str) -> Option<&'static str> {
    PAGES.iter().find(|(name, _)| *name == file).map(|(_, text)| *text)
}

/// Why `sg help` printed nothing.
#[derive(Debug, PartialEq)]
pub enum Refusal {
    /// More words than one: the command line is wrong.
    Usage,
    /// A name it does not know, and what to say about it.
    Unknown(String),
}

/// What `sg help` prints for the words after it: the list of the words with a page, the index (`index`), or the
/// page of one name. A name is looked for as it is written, then in any case, then as a command (`build` is
/// `sg build`).
pub fn text(words: &[String]) -> Result<String, Refusal> {
    match words {
        [] => Ok(overview()),
        [word] if word == "index" => Ok(index()),
        [word] => {
            let all: Vec<Name> = names().collect();
            let found = all
                .iter()
                .find(|n| n.name == word)
                .or_else(|| all.iter().find(|n| n.name.eq_ignore_ascii_case(word)))
                .or_else(|| all.iter().find(|n| n.kind == "command" && n.name.strip_prefix("sg ").is_some_and(|c| c.eq_ignore_ascii_case(word))))
                .or_else(|| all.iter().find(|n| !n.page.is_empty() && n.page.eq_ignore_ascii_case(word)))
                .or_else(|| all.iter().find(|n| n.kind == "sign" && !marks(word).is_empty() && marks(n.name) == marks(word)));
            match found {
                Some(n) => Ok(match page(n.page) {
                    Some(text) if !n.page.is_empty() => text.to_string(),
                    _ => line_of(n),
                }),
                None => Err(Refusal::Unknown(unknown(word, &all))),
            }
        }
        _ => Err(Refusal::Usage),
    }
}

/// What `sg <command> --help` prints (choice 2088): the page of the command, as `sg help sg-build` prints it.
/// `None` is `sg` itself; `update`, `remove` and `lock` are on the page of `sg add`.
pub fn of_command(command: Option<&str>) -> String {
    let name = match command {
        None => "sg".to_string(),
        Some("update" | "remove" | "lock") => "sg add".to_string(),
        Some(command) => format!("sg {command}"),
    };
    text(&[name]).unwrap_or_else(|_| unreachable!("every command is a name of the index"))
}

/// The marks of a sign, without the names standing for its parts (choice 2086): `x ?? y` is `??`, `T[]` is `[]`,
/// `xs[^1]` is `[^]`, so `sg help ??` finds it.
fn marks(name: &str) -> String {
    name.chars().filter(|c| !c.is_alphanumeric() && *c != '_' && !c.is_whitespace()).collect()
}

/// A name `sg help` has no page for yet: its line, and where the site says more.
fn line_of(n: &Name) -> String {
    let mut out = format!("NAME\n{}\n\n", wrap(&format!("{} - {}", n.name, n.summary), 5));
    out.push_str(&format!("{}\n\n", wrap(&format!("A {} of SG. sg help has no page for it yet; the site says more:", n.kind), 5)));
    out.push_str(&format!("     {}\n", n.address));
    out
}

fn overview() -> String {
    let all: Vec<Name> = names().collect();
    let mut out = String::from("SG HELP\n");
    out.push_str("     sg help <name>     the page of a word, or the line of any other name\n");
    out.push_str("     sg help index      every name of SG, with what it is in one line\n");
    out.push_str("     sg build --help    the page of a command; so does sg help sg-build\n\n");
    for (title, commands) in [("THE COMMANDS", true), ("THE WORDS WITH A PAGE", false)] {
        let with_page: Vec<&Name> = all.iter().filter(|n| !n.page.is_empty() && (n.kind == "command") == commands).collect();
        if !with_page.is_empty() {
            out.push_str(title);
            out.push('\n');
            out.push_str(&rows(&with_page));
            out.push('\n');
        }
    }
    out.push_str(&wrap("The same pages are on the site, https://sg-lang.org/reference/, and are man(1)'s sg-build(1), sg-read(7) and the others where they are installed.", 0));
    out.push('\n');
    out
}

fn index() -> String {
    let all: Vec<Name> = names().collect();
    let mut out = String::new();
    let mut parts: Vec<&str> = Vec::new();
    for n in &all {
        if !parts.contains(&n.part) {
            parts.push(n.part);
        }
    }
    for part in parts {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&part.to_uppercase());
        out.push('\n');
        let list: Vec<&Name> = all.iter().filter(|n| n.part == part).collect();
        out.push_str(&rows(&list));
    }
    out.push_str("\nsg help <name> prints the page of a name that has one, the line of any other.\n");
    out
}

/// Names and what they are, in two columns as man lays out a list; a name too long for its column has a line
/// of its own.
fn rows(list: &[&Name]) -> String {
    const MOST: usize = 14;
    let column = list.iter().map(|n| n.name.chars().count()).filter(|&w| w <= MOST).max().unwrap_or(0) + 2;
    let mut out = String::new();
    for n in list {
        let width = n.name.chars().count();
        let text = wrap(n.summary, 5 + column);
        if width + 2 <= column {
            out.push_str(&format!("     {}{}{}\n", n.name, " ".repeat(column - width), &text[5 + column..]));
        } else {
            out.push_str(&format!("     {}\n{}\n", n.name, text));
        }
    }
    out
}

/// The words of `text` in lines of at most 78 columns, each line indented by `indent`.
fn wrap(text: &str, indent: usize) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && indent + line.chars().count() + 1 + word.chars().count() > 78 {
            lines.push(format!("{}{}", " ".repeat(indent), line));
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    lines.push(format!("{}{}", " ".repeat(indent), line));
    lines.join("\n")
}

fn unknown(word: &str, all: &[Name]) -> String {
    let lower = word.to_lowercase();
    let like: Vec<&str> = all
        .iter()
        .map(|n| n.name)
        .filter(|name| {
            let name = name.to_lowercase();
            !lower.is_empty() && (name.starts_with(&lower) || name.strip_prefix("sg ").is_some_and(|c| c.starts_with(&lower)))
        })
        .take(8)
        .collect();
    let mut message = format!("SG has no name `{word}` that `sg help` knows");
    if like.is_empty() {
        message.push_str("; `sg help index` lists every name");
    } else {
        message.push_str(&format!("; names that start like it: {}", like.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")));
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_has_its_name_and_every_name_its_page() {
        for n in names() {
            assert!(!n.name.is_empty() && !n.summary.is_empty() && n.address.starts_with("https://sg-lang.org/"), "{}", n.name);
            if !n.page.is_empty() {
                assert!(page(n.page).is_some(), "{}: no file {}.txt", n.name, n.page);
            }
        }
        for (file, text) in PAGES {
            assert!(names().any(|n| n.page == *file), "{file}.txt: no name of the index has it as its page");
            assert!(text.starts_with("NAME\n"), "{file}.txt");
        }
    }

    /// Each sign is found by its marks alone, and no two signs share them.
    #[test]
    fn every_sign_by_its_marks() {
        let signs: Vec<Name> = names().filter(|n| n.kind == "sign").collect();
        assert!(signs.len() >= 8);
        for n in &signs {
            let alone = marks(n.name);
            assert!(!alone.is_empty() && signs.iter().filter(|m| marks(m.name) == alone).count() == 1, "{}", n.name);
            let found = text(std::slice::from_ref(&alone)).unwrap_or_else(|_| panic!("{alone}: not found"));
            assert_eq!(found, text(&[n.name.to_string()]).unwrap(), "{alone}");
        }
        for word in ["??", "?.", "[]", "*", "..", "[^]"] {
            assert!(text(&[word.to_string()]).is_ok(), "{word}");
        }
        assert!(text(&["+".to_string()]).is_err());
    }

    /// Each command has a page, or a line until its page is written; `update`, `remove` and `lock` are `sg add`'s.
    #[test]
    fn every_command_has_its_help() {
        let commands: Vec<Name> = names().filter(|n| n.kind == "command").collect();
        assert!(commands.len() >= 7);
        for n in &commands {
            let command = n.name.strip_prefix("sg ");
            assert_eq!(of_command(command), text(&[n.name.to_string()]).unwrap(), "{}", n.name);
            assert!(of_command(command).starts_with("NAME\n"), "{}", n.name);
        }
        for command in ["update", "remove", "lock"] {
            assert_eq!(of_command(Some(command)), of_command(Some("add")));
        }
        // A word keeps its name: `sg help test` is the word, `sg help sg-test` the command.
        assert_ne!(text(&["test".to_string()]).unwrap(), of_command(Some("test")));
    }

    #[test]
    fn rows_keep_to_78_columns() {
        for text in [overview(), index()] {
            for line in text.lines() {
                assert!(line.chars().count() <= 78, "{line}");
            }
        }
    }
}
