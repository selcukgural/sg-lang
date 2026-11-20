//! The reader of zip archives of the language's own module (choice 1512), on any bytes: they are
//! written to a file, opened by `ZipReader.open`, and each entry is read with a limit of a
//! megabyte. Each ends with the bytes or a `ZipError`, never a panic of SG or of Rust, a hang or a
//! stack run out. An input of up to 2 KB is also written by `ZipWriter` as two entries, stored and
//! deflated, and the archive must give it back.
#![no_main]

use libfuzzer_sys::fuzz_target;
use std::fmt::Write as _;

fuzz_target!(|data: &[u8]| {
    // A file of each process: the fuzzer runs several at once.
    let dir = std::env::temp_dir();
    let input = dir.join(format!("sg-fuzz-zip-{}.zip", std::process::id()));
    let output = dir.join(format!("sg-fuzz-zip-{}-out.zip", std::process::id()));
    std::fs::write(&input, data).unwrap();
    let mut bytes = String::from("List<byte> {");
    for (i, b) in data.iter().take(2048).enumerate() {
        let _ = write!(bytes, "{}{b}", if i == 0 { " " } else { ", " });
    }
    bytes.push_str(" }");
    let round = match data.len() <= 2048 {
        true => format!(
            "    var data = {bytes}
    var target = File.createText({output:?}) ?? panic(\"cannot write\")
    var writer = ZipWriter.create(target)
    var when = DateTime.fromUnixSeconds(1700000000)
    if writer.add(\"a/stored\", read data, ZipMethod.Stored, when) is Error(_) {{
        panic(\"add\")
    }}
    if writer.add(\"a/deflated\", read data, ZipMethod.Deflated, when) is Error(_) {{
        panic(\"add\")
    }}
    if writer.close() is Error(_) {{
        panic(\"close\")
    }}
    var back = ZipReader.open({output:?}) ?? panic(\"open\")
    for entry in back.entries() {{
        var opened = back.readBytes(read entry, 4096) ?? panic(\"read\")
        assert(opened == data)
    }}
"
        ),
        false => String::new(),
    };
    let program = format!(
        "void main() {{
    match ZipReader.open({input:?}) {{
        Ok(zip) => {{
            var reader = zip
            for entry in reader.entries() {{
                match reader.readBytes(read entry, 1 << 20) {{
                    Ok(opened) => _ = opened.length()
                    Error(e) => _ = $\"{{e}}\".toString()
                }}
            }}
        }}
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
