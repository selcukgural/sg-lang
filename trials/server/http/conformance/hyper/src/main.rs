//! The reference server of the conformance comparison (choices 1772, 1794): hyper 1's server of
//! HTTP/1.1 answering every request 200 with `METHOD TARGET 1.MINOR host 'HOST' body LENGTH`, the
//! body read only for a target that starts with `/body`, as `../echo.sg` answers it, with the
//! limits of SG's module where hyper has them: a head of 8 KB, 100 headers, a body of 1 MB.
//!
//!     echo PORT

use bytes::Bytes;
use http_body_util::{BodyExt, Full, LengthLimitError, Limited};
use hyper::body::Body;
use hyper::body::Incoming;
use hyper::header::{CONTENT_TYPE, HOST};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode, Version};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use tokio::net::TcpListener;

const LIMIT: usize = 1 << 20;

fn refusal(status: StatusCode) -> Response<Full<Bytes>> {
    let text = status.canonical_reason().unwrap_or("");
    Response::builder().status(status).header(CONTENT_TYPE, "text/plain").body(Full::new(Bytes::from_static(text.as_bytes()))).unwrap()
}

async fn answer(request: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let method = request.method().to_string();
    let target = request.uri().to_string();
    let minor = if request.version() == Version::HTTP_10 { 0 } else { 1 };
    // The URL's host when the target is one, as RFC 9112 asks; `Host`'s otherwise.
    let host = match request.uri().authority() {
        Some(authority) => authority.to_string(),
        None => request.headers().get(HOST).map(|h| String::from_utf8_lossy(h.as_bytes()).into_owned()).unwrap_or_default(),
    };
    let mut length = 0;
    if target.starts_with("/body") {
        // A length over the limit is refused before the body is read, as axum's `DefaultBodyLimit`
        // does; a body that is not HTTP is the client's fault.
        if request.body().size_hint().lower() > LIMIT as u64 {
            return Ok(refusal(StatusCode::PAYLOAD_TOO_LARGE));
        }
        match Limited::new(request.into_body(), LIMIT).collect().await {
            Ok(body) => length = body.to_bytes().len(),
            Err(e) if e.downcast_ref::<LengthLimitError>().is_some() => return Ok(refusal(StatusCode::PAYLOAD_TOO_LARGE)),
            Err(_) => return Ok(refusal(StatusCode::BAD_REQUEST)),
        }
    }
    let text = format!("{method} {target} 1.{minor} host '{host}' body {length}");
    Ok(Response::builder().header(CONTENT_TYPE, "text/plain").body(Full::new(Bytes::from(text))).unwrap())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let port: u16 = match args.get(1).and_then(|p| p.parse().ok()) {
        Some(port) if args.len() == 2 => port,
        _ => {
            eprintln!("usage: echo PORT");
            std::process::exit(2);
        }
    };
    let listener = TcpListener::bind(("127.0.0.1", port)).await.unwrap_or_else(|e| {
        eprintln!("echo: {e}");
        std::process::exit(1);
    });
    loop {
        if let Ok((conn, _)) = listener.accept().await {
            tokio::spawn(async move {
                let _ = http1::Builder::new()
                    .max_buf_size(8192)
                    .max_headers(100)
                    .serve_connection(TokioIo::new(conn), service_fn(answer))
                    .await;
            });
        }
    }
}
