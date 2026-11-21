//! `unicode`: `std`'s categories and normal forms of Unicode measured against ICU4X's
//! `icu_properties` and `icu_normalizer`, and with the feature `unicode-normalization` against the
//! crate of that name (choices 1553, 1629-1637, 1659, 1661, 1662). Four ways of using them, each on
//! every line of FILE, without its end (`\n` or `\r\n`):
//!
//!     unicode category FILE      the general category of each character; how many characters of
//!                                each category, in the order of SG's `UnicodeCategory`
//!     unicode nfc FILE OUT       each line in NFC, written into OUT with `\n` after it
//!     unicode nfd FILE OUT       each line in NFD, the same way
//!     unicode check FILE         whether each line is in NFC; how many lines, and how many are
//!
//! `nfc` and `nfd` print how many lines they wrote. A file that cannot be read or written is told on
//! the standard error, with exit code 1; a wrong use, with 2.

use icu_properties::CodePointMapData;
use icu_properties::props::GeneralCategory;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::process::exit;

const USAGE: &str = "usage: unicode category FILE | nfc FILE OUT | nfd FILE OUT | check FILE";

const ABBREVIATIONS: [&str; 29] = [
    "Lu", "Ll", "Lt", "Lm", "Lo", "Mn", "Mc", "Me", "Nd", "Nl", "No", "Pc", "Pd", "Ps", "Pe", "Pi", "Pf", "Po", "Sm",
    "Sc", "Sk", "So", "Zs", "Zl", "Zp", "Cc", "Cf", "Co", "Cn",
];

/// The place of a category in the counts, in the order of SG's `UnicodeCategory`. A `char` is
/// never a surrogate.
fn place(c: GeneralCategory) -> usize {
    use GeneralCategory::*;
    match c {
        UppercaseLetter => 0,
        LowercaseLetter => 1,
        TitlecaseLetter => 2,
        ModifierLetter => 3,
        OtherLetter => 4,
        NonspacingMark => 5,
        SpacingMark => 6,
        EnclosingMark => 7,
        DecimalNumber => 8,
        LetterNumber => 9,
        OtherNumber => 10,
        ConnectorPunctuation => 11,
        DashPunctuation => 12,
        OpenPunctuation => 13,
        ClosePunctuation => 14,
        InitialPunctuation => 15,
        FinalPunctuation => 16,
        OtherPunctuation => 17,
        MathSymbol => 18,
        CurrencySymbol => 19,
        ModifierSymbol => 20,
        OtherSymbol => 21,
        SpaceSeparator => 22,
        LineSeparator => 23,
        ParagraphSeparator => 24,
        Control => 25,
        Format => 26,
        PrivateUse => 27,
        Unassigned | Surrogate => 28,
    }
}

/// Each line of `path` given to `each`, without its end.
fn lines(path: &str, mut each: impl FnMut(&str) -> io::Result<()>) -> io::Result<()> {
    let mut reader = BufReader::with_capacity(65536, File::open(path)?);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let text = line.strip_suffix('\n').map(|t| t.strip_suffix('\r').unwrap_or(t)).unwrap_or(&line);
        each(text)?;
    }
}

fn category(path: &str) -> io::Result<()> {
    let map = CodePointMapData::<GeneralCategory>::new();
    let mut counts = [0u64; 29];
    lines(path, |line| {
        for c in line.chars() {
            counts[place(map.get(c))] += 1;
        }
        Ok(())
    })?;
    let mut out = String::new();
    for (name, count) in ABBREVIATIONS.iter().zip(counts) {
        out.push_str(&format!("{name} {count}\n"));
    }
    print!("{out}");
    Ok(())
}

#[cfg(not(feature = "unicode-normalization"))]
fn normalized_into(line: &str, composed: bool, out: &mut String) {
    use icu_normalizer::{ComposingNormalizerBorrowed, DecomposingNormalizerBorrowed};
    if composed {
        ComposingNormalizerBorrowed::new_nfc().normalize_to(line, out).unwrap();
    } else {
        DecomposingNormalizerBorrowed::new_nfd().normalize_to(line, out).unwrap();
    }
}

#[cfg(feature = "unicode-normalization")]
fn normalized_into(line: &str, composed: bool, out: &mut String) {
    use unicode_normalization::UnicodeNormalization;
    if composed {
        out.extend(line.nfc());
    } else {
        out.extend(line.nfd());
    }
}

#[cfg(not(feature = "unicode-normalization"))]
fn is_nfc(line: &str) -> bool {
    icu_normalizer::ComposingNormalizerBorrowed::new_nfc().is_normalized(line)
}

#[cfg(feature = "unicode-normalization")]
fn is_nfc(line: &str) -> bool {
    unicode_normalization::is_nfc(line)
}

fn normalize(path: &str, out: &str, composed: bool) -> io::Result<()> {
    let mut target = BufWriter::with_capacity(65536, File::create(out)?);
    let mut text = String::new();
    let mut count = 0u64;
    lines(path, |line| {
        text.clear();
        normalized_into(line, composed, &mut text);
        text.push('\n');
        count += 1;
        target.write_all(text.as_bytes())
    })?;
    target.flush()?;
    println!("lines {count}");
    Ok(())
}

fn check(path: &str) -> io::Result<()> {
    let (mut count, mut normalized) = (0u64, 0u64);
    lines(path, |line| {
        if is_nfc(line) {
            normalized += 1;
        }
        count += 1;
        Ok(())
    })?;
    println!("lines {count}\nnormalized {normalized}");
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let one = args.len() == 2 && (args[0] == "category" || args[0] == "check");
    let two = args.len() == 3 && (args[0] == "nfc" || args[0] == "nfd");
    if !one && !two {
        eprintln!("{USAGE}");
        exit(2);
    }
    let result = match args[0].as_str() {
        "category" => category(&args[1]),
        "check" => check(&args[1]),
        mode => normalize(&args[1], &args[2], mode == "nfc"),
    };
    if let Err(e) = result {
        eprintln!("unicode: {}: {e}", args[1]);
        exit(1);
    }
}
