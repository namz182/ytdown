use serde::Deserialize;
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    time::Duration,
};
use tauri::AppHandle;

#[derive(Deserialize)]
struct DownloadRequest {
    url: String,
}

pub fn start(app: AppHandle) {
    for address in ["127.0.0.1:5050", "[::1]:5050"] {
        let app = app.clone();
        let address = address.to_string();
        let _ = std::thread::Builder::new()
            .name(format!("extension-http-{address}"))
            .spawn(move || {
                let listener = match TcpListener::bind(&address) {
                    Ok(listener) => listener,
                    Err(error) => {
                        eprintln!(
                            "Could not start browser extension service on {address}: {error}"
                        );
                        return;
                    }
                };
                for stream in listener.incoming().flatten() {
                    let app = app.clone();
                    std::thread::spawn(move || handle(stream, app));
                }
            });
    }
}

fn handle(mut stream: TcpStream, app: AppHandle) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut request = Vec::with_capacity(4096);
    let mut buffer = [0_u8; 4096];
    let mut header_end = None;
    let mut content_length = 0_usize;
    loop {
        let Ok(read) = stream.read(&mut buffer) else {
            return;
        };
        if read == 0 {
            return;
        }
        request.extend_from_slice(&buffer[..read]);
        if header_end.is_none() {
            if let Some(index) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                header_end = Some(index + 4);
                let headers = String::from_utf8_lossy(&request[..index]);
                content_length = headers
                    .lines()
                    .filter_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().ok())
                            .flatten()
                    })
                    .next()
                    .unwrap_or(0);
            }
        }
        if header_end.is_some_and(|end| request.len() >= end + content_length) {
            break;
        }
        if request.len() > 1024 * 1024 {
            respond(
                &mut stream,
                413,
                json!({"status":"error","message":"Request too large"}),
            );
            return;
        }
    }
    let Some(end) = header_end else { return };
    let headers = String::from_utf8_lossy(&request[..end]);
    let mut parts = headers
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    if method == "OPTIONS" {
        let response = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: POST, GET, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nAccess-Control-Allow-Private-Network: true\r\nAccess-Control-Max-Age: 86400\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(response.as_bytes());
    } else if method == "GET" && path == "/status" {
        respond(&mut stream, 200, json!({"status":"running"}));
    } else if method == "POST" && path == "/download" {
        match serde_json::from_slice::<DownloadRequest>(&request[end..end + content_length]) {
            Ok(payload) => {
                let (sender, receiver) = std::sync::mpsc::channel();
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let result = crate::service::enqueue_browser_link(handle, payload.url).await;
                    let _ = sender.send(result);
                });
                match receiver.recv_timeout(Duration::from_secs(180)) {
                    Ok(Ok(_)) => respond(
                        &mut stream,
                        200,
                        json!({"status":"success","message":"Download started"}),
                    ),
                    Ok(Err(message)) => respond(
                        &mut stream,
                        400,
                        json!({"status":"error","message":message}),
                    ),
                    Err(_) => respond(
                        &mut stream,
                        504,
                        json!({"status":"error","message":"Timed out preparing download"}),
                    ),
                }
            }
            Err(_) => respond(
                &mut stream,
                400,
                json!({"status":"error","message":"Expected JSON with a YouTube url"}),
            ),
        }
    } else {
        respond(
            &mut stream,
            404,
            json!({"status":"error","message":"Not found"}),
        );
    }
}

fn respond(stream: &mut TcpStream, status: u16, body: serde_json::Value) {
    let body = body.to_string();
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        413 => "Payload Too Large",
        504 => "Gateway Timeout",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: POST, GET, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nAccess-Control-Allow-Private-Network: true\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}
