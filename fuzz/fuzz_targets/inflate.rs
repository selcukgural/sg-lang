//! The decompressors of the language's own module (choice 1492), on any bytes: the first byte
//! chooses the format (DEFLATE, zlib or gzip), the rest is opened with a limit of a megabyte. Each
//! ends with the bytes or a `CompressError`, never a panic of SG or of Rust, a hang or a stack run
//! out. An input of up to 2 KB is also compressed at the level its second byte chooses, and must
//! open into itself.
#![no_main]

use libfuzzer_sys::fuzz_target;
use std::fmt::Write as _;

fuzz_target!(|data: &[u8]| {
    let Some((&first, rest)) = data.split_first() else { return };
    let format = ["Deflate", "Zlib", "Gzip"][first as usize % 3];
    let mut bytes = String::from("List<byte> {");
    for (i, b) in rest.iter().enumerate() {
        let _ = write!(bytes, "{}{b}", if i == 0 { " " } else { ", " });
    }
    bytes.push_str(" }");
    let level = rest.first().map_or(6, |b| b % 10);
    let round = match rest.len() <= 2048 {
        true => format!(
            "    var packed = {format}.compress(read data, {level})
    match {format}.decompress(read packed, data.length()) {{
        Ok(back) => assert(back == data)
        Error(e) => panic($\"{{e}}\")
    }}
"
        ),
        false => String::new(),
    };
    let program = format!(
        "void main() {{
    var data = {bytes}
    match {format}.decompress(read data, 1 << 20) {{
        Ok(opened) => _ = opened.length()
        Error(e) => _ = $\"{{e}}\".toString()
    }}
{round}}}
"
    );
    let mut out = std::io::sink();
    if let Err(stop) = sg::run(&program, &mut out) {
        match stop {
            sg::Stop::Diag(diag) => panic!("{}", diag.render(&program, sg_fuzz::PATH)),
            _ => panic!("the program stopped"),
        }
    }
});
