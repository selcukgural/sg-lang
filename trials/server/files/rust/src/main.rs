//! The file server of the server lane in Rust (choices 1861-1871): GET and HEAD of the files under a
//! folder, over HTTP/1.1 with connections kept open, by hyper 1's server on `tokio`, as
//! `../c/files.c` describes it for all four versions. hyper reads and checks the request, keeps or
//! closes the connection as `Connection` says, adds `date`, and leaves the body out for HEAD.
//!
//!     files PORT ROOT
//!
//! The disk is asked through `tokio::fs` (`metadata`, `File::open`), whose calls go to the threads of
//! `spawn_blocking`, and the body is the file read by `ReaderStream` in pieces of 64 KB, no more
//! read than the answer's length.

use bytes::Bytes;
use futures_util::TryStreamExt;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::header::{HeaderValue, ACCEPT_RANGES, ALLOW, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, IF_MODIFIED_SINCE, IF_RANGE, LAST_MODIFIED, LOCATION, RANGE};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::io::{ErrorKind, SeekFrom};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;

type Body = BoxBody<Bytes, std::io::Error>;

const PIECE: usize = 65536;
const FAR: i64 = 1 << 62;

// ---- HTTP dates

const DAYS: &[u8] = b"MonTueWedThuFriSatSun";
const MONTHS: &[u8] = b"JanFebMarAprMayJunJulAugSepOctNovDec";

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = y - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, usize, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m as usize, d)
}

/// `Sun, 06 Nov 1994 08:49:37 GMT`
fn http_date(seconds: i64) -> String {
    let days = seconds.div_euclid(86400);
    let rest = seconds.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let w = (days + 3).rem_euclid(7) as usize;
    let day = std::str::from_utf8(&DAYS[w * 3..w * 3 + 3]).unwrap();
    let month = std::str::from_utf8(&MONTHS[(m - 1) * 3..(m - 1) * 3 + 3]).unwrap();
    format!("{day}, {d:02} {month} {y:04} {:02}:{:02}:{:02} GMT", rest / 3600, rest / 60 % 60, rest % 60)
}

struct Text<'a> {
    t: &'a [u8],
    at: usize,
}

impl Text<'_> {
    fn digits(&mut self, count: usize) -> Option<i64> {
        let mut v = 0;
        for _ in 0..count {
            let c = *self.t.get(self.at).filter(|c| c.is_ascii_digit())?;
            v = v * 10 + i64::from(c - b'0');
            self.at += 1;
        }
        Some(v)
    }

    fn expect(&mut self, c: u8) -> Option<()> {
        (self.t.get(self.at) == Some(&c)).then(|| self.at += 1)
    }

    fn name(&mut self, names: &[u8], width: usize) -> Option<i64> {
        let here = self.t.get(self.at..self.at + width)?;
        let i = names.chunks(width).position(|name| name == here)?;
        self.at += width;
        Some(i as i64)
    }
}

/// An HTTP date (RFC 9110: IMF-fixdate, RFC 850, asctime) into seconds since 1970, as SG's
/// `DateTime.parseHttpDate` reads it.
fn parse_http_date(t: &[u8]) -> Option<i64> {
    let mut x = Text { t, at: 0 };
    let (named, day, month, mut year);
    let asctime = t.len() > 3 && t[3] == b' ';
    if t.len() > 3 && t[3] == b',' {
        named = x.name(DAYS, 3)?;
        x.expect(b',')?;
        x.expect(b' ')?;
        day = x.digits(2)?;
        x.expect(b' ')?;
        month = x.name(MONTHS, 3)? + 1;
        x.expect(b' ')?;
        year = x.digits(4)?;
        x.expect(b' ')?;
    } else if asctime {
        named = x.name(DAYS, 3)?;
        x.expect(b' ')?;
        month = x.name(MONTHS, 3)? + 1;
        x.expect(b' ')?;
        day = if x.t.get(x.at) == Some(&b' ') {
            x.at += 1;
            x.digits(1)?
        } else {
            x.digits(2)?
        };
        x.expect(b' ')?;
        year = 0;
    } else {
        named = x.name(DAYS, 3)?;
        let rest: &[u8] = [&b"day"[..], b"sday", b"nesday", b"rsday", b"day", b"urday", b"day"][named as usize];
        if x.t.get(x.at..x.at + rest.len()) != Some(rest) {
            return None;
        }
        x.at += rest.len();
        x.expect(b',')?;
        x.expect(b' ')?;
        day = x.digits(2)?;
        x.expect(b'-')?;
        month = x.name(MONTHS, 3)? + 1;
        x.expect(b'-')?;
        let yy = x.digits(2)?;
        year = yy + if yy < 70 { 2000 } else { 1900 };
        x.expect(b' ')?;
    }
    let hour = x.digits(2)?;
    x.expect(b':')?;
    let minute = x.digits(2)?;
    x.expect(b':')?;
    let second = x.digits(2)?;
    x.expect(b' ')?;
    if asctime {
        year = x.digits(4)?;
    } else {
        x.expect(b'G')?;
        x.expect(b'M')?;
        x.expect(b'T')?;
    }
    if x.at != t.len() {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let in_month = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][month as usize - 1];
    if day < 1 || day > in_month || hour > 23 || minute > 59 || second > 59 || !(1000..=3000).contains(&year) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    let s = days * 86400 + hour * 3600 + minute * 60 + second;
    if !(-9223372036..=9223372036).contains(&s) || (days + 3).rem_euclid(7) != named {
        return None;
    }
    Some(s)
}

// ---- what is asked

fn hex(c: u8) -> Option<u8> {
    (c as char).to_digit(16).map(|v| v as u8)
}

/// The path of `target`, percent-decoded and checked as `../c/files.c` says; `None` for 400.
fn decode_path(target: &str) -> Option<String> {
    let raw = target.split('?').next().unwrap_or("").as_bytes();
    if raw.first() != Some(&b'/') || !target.bytes().all(|c| (0x21..=0x7e).contains(&c)) {
        return None;
    }
    let mut path = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let mut c = raw[i];
        if c == b'%' {
            if i + 2 >= raw.len() {
                return None;
            }
            c = hex(raw[i + 1])? << 4 | hex(raw[i + 2])?;
            if c == b'/' {
                return None;
            }
            i += 2;
        }
        if c == 0 || c == b'\\' {
            return None;
        }
        path.push(c);
        i += 1;
    }
    let segments: Vec<&[u8]> = path[1..].split(|&c| c == b'/').collect();
    let last = segments.len() - 1;
    for (k, segment) in segments.iter().enumerate() {
        if (segment.is_empty() && k != last) || *segment == b"." || *segment == b".." {
            return None;
        }
    }
    String::from_utf8(path).ok()
}

fn type_of(path: &str) -> &'static str {
    let slash = path.rfind('/').unwrap_or(0);
    let dot = path.rfind('.').unwrap_or(0);
    if dot <= slash {
        return "application/octet-stream";
    }
    match path[dot + 1..].to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        _ => "application/octet-stream",
    }
}

fn digits_far(v: &[u8], at: &mut usize) -> Option<i64> {
    let start = *at;
    let mut x: i64 = 0;
    while *at < v.len() && v[*at].is_ascii_digit() {
        x = if x > FAR / 10 { FAR } else { (x * 10 + i64::from(v[*at] - b'0')).min(FAR) };
        *at += 1;
    }
    (*at > start).then_some(x)
}

enum Wanted {
    Whole,
    From(i64, i64),
    Last(i64),
}

fn range_of(v: &[u8]) -> Wanted {
    let Some(rest) = v.strip_prefix(b"bytes=") else { return Wanted::Whole };
    let mut at = 0;
    let first = digits_far(rest, &mut at);
    if rest.get(at) != Some(&b'-') {
        return Wanted::Whole;
    }
    at += 1;
    let last = digits_far(rest, &mut at);
    if at != rest.len() {
        return Wanted::Whole;
    }
    match (first, last) {
        (None, Some(count)) => Wanted::Last(count),
        (Some(first), None) => Wanted::From(first, FAR),
        (Some(first), Some(last)) if first <= last => Wanted::From(first, last),
        _ => Wanted::Whole,
    }
}

// ---- answers

fn simple(status: StatusCode, body: &'static str) -> Response<Body> {
    let mut response = Response::new(Full::new(Bytes::from_static(body.as_bytes())).map_err(|never| match never {}).boxed());
    *response.status_mut() = status;
    response.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
    response
}

fn failed(cause: &std::io::Error) -> Response<Body> {
    match cause.kind() {
        ErrorKind::PermissionDenied => simple(StatusCode::FORBIDDEN, "forbidden"),
        _ => simple(StatusCode::NOT_FOUND, "not found"),
    }
}

async fn answer(request: Request<Incoming>, root: Arc<String>) -> Result<Response<Body>, std::convert::Infallible> {
    let head = request.method() == Method::HEAD;
    if request.method() != Method::GET && !head {
        let mut response = simple(StatusCode::METHOD_NOT_ALLOWED, "method not allowed");
        response.headers_mut().insert(ALLOW, HeaderValue::from_static("GET, HEAD"));
        return Ok(response);
    }
    let uri = request.uri();
    let target = uri.path_and_query().map_or("", |p| p.as_str());
    let Some(path) = uri.scheme().is_none().then(|| decode_path(target)).flatten() else {
        return Ok(simple(StatusCode::BAD_REQUEST, "bad request"));
    };
    let mut full = format!("{root}{path}");
    if path.ends_with('/') {
        full.push_str("index.html");
    }
    let info = match tokio::fs::metadata(&full).await {
        Ok(info) => info,
        Err(cause) => return Ok(failed(&cause)),
    };
    if info.is_dir() {
        if path.ends_with('/') {
            return Ok(simple(StatusCode::NOT_FOUND, "not found"));
        }
        let mut response = simple(StatusCode::MOVED_PERMANENTLY, "moved permanently");
        let written = format!("{}/", uri.path());
        response.headers_mut().insert(LOCATION, HeaderValue::from_str(&written).expect("a path of visible ASCII"));
        return Ok(response);
    }
    if !info.is_file() {
        return Ok(simple(StatusCode::NOT_FOUND, "not found"));
    }
    let mtime = match info.modified().map(|t| t.duration_since(UNIX_EPOCH)) {
        Ok(Ok(since)) => since.as_secs() as i64,
        Ok(Err(before)) => -(before.duration().as_secs_f64().ceil() as i64),
        Err(_) => 0,
    };
    let modified = HeaderValue::from_str(&http_date(mtime)).expect("a date");
    let headers = request.headers();
    if headers.get(IF_MODIFIED_SINCE).and_then(|v| parse_http_date(v.as_bytes())).is_some_and(|since| mtime <= since) {
        let mut response = Response::new(Empty::new().map_err(|never| match never {}).boxed());
        *response.status_mut() = StatusCode::NOT_MODIFIED;
        response.headers_mut().insert(LAST_MODIFIED, modified);
        return Ok(response);
    }
    let size = info.len() as i64;
    let (mut start, mut end) = (0, size - 1);
    let wanted = match (headers.get(RANGE), headers.contains_key(IF_RANGE)) {
        (Some(v), false) => range_of(v.as_bytes()),
        _ => Wanted::Whole,
    };
    let partial = !matches!(wanted, Wanted::Whole);
    match wanted {
        Wanted::Whole => {}
        Wanted::From(first, last) if first < size => {
            start = first;
            end = end.min(last);
        }
        Wanted::Last(count) if count > 0 && size > 0 => start = size - count.min(size),
        _ => {
            let mut response = simple(StatusCode::RANGE_NOT_SATISFIABLE, "range not satisfiable");
            response.headers_mut().insert(CONTENT_RANGE, HeaderValue::from_str(&format!("bytes */{size}")).expect("a range"));
            return Ok(response);
        }
    }
    let mut file = match tokio::fs::File::open(&full).await {
        Ok(file) => file,
        Err(cause) => return Ok(failed(&cause)),
    };
    let length = end - start + 1;
    let body = if head || length == 0 {
        Empty::new().map_err(|never| match never {}).boxed()
    } else {
        if start > 0 && file.seek(SeekFrom::Start(start as u64)).await.is_err() {
            return Ok(simple(StatusCode::NOT_FOUND, "not found"));
        }
        let pieces = ReaderStream::with_capacity(file.take(length as u64), PIECE);
        StreamBody::new(pieces.map_ok(Frame::data)).boxed()
    };
    let mut response = Response::new(body);
    *response.status_mut() = if partial { StatusCode::PARTIAL_CONTENT } else { StatusCode::OK };
    let h = response.headers_mut();
    h.insert(CONTENT_TYPE, HeaderValue::from_static(type_of(&full)));
    h.insert(CONTENT_LENGTH, HeaderValue::from(length));
    h.insert(LAST_MODIFIED, modified);
    h.insert(ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if partial {
        h.insert(CONTENT_RANGE, HeaderValue::from_str(&format!("bytes {start}-{end}/{size}")).expect("a range"));
    }
    Ok(response)
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (port, root): (u16, String) = match (args.get(1).and_then(|p| p.parse().ok()), args.get(2)) {
        (Some(port), Some(root)) if args.len() == 3 => (port, root.clone()),
        _ => {
            eprintln!("usage: files PORT ROOT");
            std::process::exit(2);
        }
    };
    let mut root = root;
    while root.len() > 1 && root.ends_with('/') {
        root.pop();
    }
    if !std::path::Path::new(&root).is_dir() {
        eprintln!("files: {root} is not a folder");
        std::process::exit(2);
    }
    let root = Arc::new(root);
    let listener = match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("files: {e}");
            std::process::exit(1);
        }
    };
    loop {
        match listener.accept().await {
            Ok((conn, _)) => {
                let _ = conn.set_nodelay(true);
                let root = root.clone();
                // A connection that fails ends alone.
                tokio::spawn(async move {
                    let service = service_fn(move |request| answer(request, root.clone()));
                    let _ = http1::Builder::new().serve_connection(TokioIo::new(conn), service).await;
                });
            }
            // Too many open files, say: try again soon.
            Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }
}
