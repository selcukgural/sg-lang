//! The HTTP server of the server lane in Rust (choice 1706): GET /ping answered with "pong" over
//! HTTP/1.1 with connections kept open, by hyper 1's server of HTTP/1.1 on `tokio`, as
//! `../c/http.c` answers it: GET or HEAD of /ping is 200 with `pong`, another target 404, another
//! method 405. hyper reads and checks the request itself, keeps or closes the connection as
//! `Connection` says, and adds a `date` to every answer.
//!
//!     http PORT
//!
//! Each connection is a task of `tokio::spawn`, given TCP_NODELAY, as a server is.

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::header::{ALLOW, CONTENT_TYPE};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::time::Duration;
use tokio::net::TcpListener;

async fn answer(request: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let (status, body) = match (request.method(), request.uri().path()) {
        (&Method::GET | &Method::HEAD, "/ping") => (StatusCode::OK, "pong"),
        (&Method::GET | &Method::HEAD, _) => (StatusCode::NOT_FOUND, "not found"),
        _ => (StatusCode::METHOD_NOT_ALLOWED, "method not allowed"),
    };
    let mut response = Response::builder().status(status).header(CONTENT_TYPE, "text/plain");
    if status == StatusCode::METHOD_NOT_ALLOWED {
        response = response.header(ALLOW, "GET, HEAD");
    }
    Ok(response.body(Full::new(Bytes::from_static(body.as_bytes()))).unwrap())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let port: u16 = match args.get(1).and_then(|p| p.parse().ok()) {
        Some(port) if args.len() == 2 => port,
        _ => {
            eprintln!("usage: http PORT");
            std::process::exit(2);
        }
    };
    let listener = match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("http: {e}");
            std::process::exit(1);
        }
    };
    loop {
        match listener.accept().await {
            Ok((conn, _)) => {
                let _ = conn.set_nodelay(true);
                // A connection that fails ends alone.
                tokio::spawn(async move {
                    let _ = http1::Builder::new().serve_connection(TokioIo::new(conn), service_fn(answer)).await;
                });
            }
            // Too many open files, say: try again soon.
            Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
        }
    }
}
