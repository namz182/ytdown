use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

pub struct MediaServer {
    port: u16,
    files: Arc<Mutex<HashMap<String, PathBuf>>>,
}

impl MediaServer {
    pub fn start() -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let files = Arc::new(Mutex::new(HashMap::new()));
        let shared = Arc::clone(&files);
        std::thread::Builder::new()
            .name("media-http".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    if let Ok(stream) = stream {
                        let files = Arc::clone(&shared);
                        std::thread::spawn(move || serve(stream, &files));
                    }
                }
            })?;
        Ok(Self { port, files })
    }

    pub fn url_for(&self, path: PathBuf) -> String {
        let token = uuid::Uuid::new_v4().simple().to_string();
        self.files.lock().unwrap().insert(token.clone(), path);
        format!("http://127.0.0.1:{}/media/{token}", self.port)
    }
}

fn serve(mut stream: TcpStream, files: &Arc<Mutex<HashMap<String, PathBuf>>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
    let mut reader = BufReader::new(&mut stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() || request_line.len() > 4096 {
        return;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    let mut range = None;
    let mut header = String::new();
    for _ in 0..64 {
        header.clear();
        if reader.read_line(&mut header).is_err() || header.len() > 8192 {
            return;
        }
        if header == "\r\n" || header == "\n" {
            break;
        }
        if let Some(value) = header
            .strip_prefix("Range:")
            .or_else(|| header.strip_prefix("range:"))
        {
            range = Some(value.trim().to_owned());
        }
    }
    drop(reader);
    if !matches!(method, "GET" | "HEAD") {
        respond_error(&mut stream, "405 Method Not Allowed");
        return;
    }
    let Some(token) = path.strip_prefix("/media/") else {
        respond_error(&mut stream, "404 Not Found");
        return;
    };
    let Some(file_path) = files.lock().unwrap().get(token).cloned() else {
        respond_error(&mut stream, "404 Not Found");
        return;
    };
    let Ok(mut file) = File::open(&file_path) else {
        respond_error(&mut stream, "404 Not Found");
        return;
    };
    let Ok(size) = file.metadata().map(|metadata| metadata.len()) else {
        respond_error(&mut stream, "500 Internal Server Error");
        return;
    };
    let (start, end, partial) = match range.as_deref() {
        Some(value) => match parse_range(value, size) {
            Some((start, end)) => (start, end, true),
            None => {
                let response = format!(
                    "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{size}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.write_all(response.as_bytes());
                return;
            }
        },
        None => (0, size.saturating_sub(1), false),
    };
    let length = if size == 0 { 0 } else { end - start + 1 };
    let mime = match file_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "mp4" | "m4v" | "mov" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "wav" => "audio/wav",
        "ogg" | "opus" => "audio/ogg",
        _ => "application/octet-stream",
    };
    let status = if partial {
        "206 Partial Content"
    } else {
        "200 OK"
    };
    let content_range = if partial {
        format!("Content-Range: bytes {start}-{end}/{size}\r\n")
    } else {
        String::new()
    };
    let headers = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {length}\r\nAccept-Ranges: bytes\r\n{content_range}Cache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nCross-Origin-Resource-Policy: cross-origin\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(headers.as_bytes()).is_err() || method == "HEAD" || length == 0 {
        return;
    }
    if file.seek(SeekFrom::Start(start)).is_ok() {
        let _ = io::copy(&mut file.take(length), &mut stream);
    }
}

fn respond_error(stream: &mut TcpStream, status: &str) {
    let response = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let _ = stream.write_all(response.as_bytes());
}

fn parse_range(header: &str, size: u64) -> Option<(u64, u64)> {
    let value = header.strip_prefix("bytes=")?;
    if value.contains(',') || size == 0 {
        return None;
    }
    let (first, last) = value.split_once('-')?;
    if first.is_empty() {
        let suffix: u64 = last.parse().ok()?;
        if suffix == 0 {
            return None;
        }
        return Some((size.saturating_sub(suffix), size - 1));
    }
    let start: u64 = first.parse().ok()?;
    if start >= size {
        return None;
    }
    let end = if last.is_empty() {
        size - 1
    } else {
        last.parse::<u64>().ok()?.min(size - 1)
    };
    (start <= end).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::{MediaServer, parse_range};
    use std::{
        io::{Read, Write},
        net::TcpStream,
    };

    #[test]
    fn byte_ranges_support_seeking_and_suffixes() {
        assert_eq!(parse_range("bytes=5-", 10), Some((5, 9)));
        assert_eq!(parse_range("bytes=2-4", 10), Some((2, 4)));
        assert_eq!(parse_range("bytes=-3", 10), Some((7, 9)));
        assert_eq!(parse_range("bytes=10-", 10), None);
    }

    #[test]
    fn serves_authorized_media_with_partial_content() {
        let path = std::env::temp_dir().join(format!("ytdown-media-{}.mp3", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"0123456789").unwrap();
        let server = MediaServer::start().unwrap();
        let url = server.url_for(path.clone());
        let address = url.strip_prefix("http://").unwrap();
        let (host, route) = address.split_once('/').unwrap();
        let mut stream = TcpStream::connect(host).unwrap();
        write!(
            stream,
            "GET /{route} HTTP/1.1\r\nHost: {host}\r\nRange: bytes=3-5\r\n\r\n"
        )
        .unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 206 Partial Content"));
        assert!(response.contains("Content-Range: bytes 3-5/10"));
        assert!(response.ends_with("\r\n\r\n345"));
        std::fs::remove_file(path).unwrap();
    }
}
