//! A small load generator for a running server: `cargo run --release --example bench -- <host:port>`.
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

struct Connection {
    stream: TcpStream,
    buffer: Vec<u8>,
}

impl Connection {
    async fn open(addr: &str) -> Connection {
        let stream = TcpStream::connect(addr).await.expect("the server is reachable");
        stream.set_nodelay(true).ok();
        Connection { stream, buffer: Vec::with_capacity(8192) }
    }

    async fn request(&mut self, addr: &str, method: &str, path: &str, headers: &str, body: &[u8]) -> (u16, String, usize) {
        let head = format!(
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Length: {}\r\n{headers}\r\n",
            body.len()
        );
        self.stream.write_all(head.as_bytes()).await.unwrap();
        self.stream.write_all(body).await.unwrap();

        self.buffer.clear();
        let mut chunk = vec![0u8; 65536];
        let mut parsed: Option<(usize, String, usize)> = None;
        loop {
            if parsed.is_none() {
                if let Some(end) = self.buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&self.buffer[..end]).to_string();
                    let length = head
                        .lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                        .unwrap_or(0);
                    parsed = Some((end, head, length));
                }
            }
            if let Some((end, head, length)) = &parsed {
                if self.buffer.len() >= end + 4 + length {
                    let status = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                    return (status, head.clone(), *length);
                }
            }
            let read = self.stream.read(&mut chunk).await.unwrap();
            assert!(read > 0, "the server closed the connection");
            self.buffer.extend_from_slice(&chunk[..read]);
        }
    }
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    sorted[((sorted.len() as f64 - 1.0) * p) as usize]
}

async fn run(addr: String, cookie: String, workers: usize, seconds: u64, label: &str, method: &str, path: &str, body: Vec<u8>) {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut tasks = Vec::new();
    for _ in 0..workers {
        let (addr, cookie, path, body, method) = (addr.clone(), cookie.clone(), path.to_string(), body.clone(), method.to_string());
        tasks.push(tokio::spawn(async move {
            let mut connection = Connection::open(&addr).await;
            let headers = format!("Cookie: dc_session={cookie}\r\nContent-Type: application/x-www-form-urlencoded\r\n");
            let mut latencies = Vec::new();
            let mut failures = 0u64;
            while Instant::now() < deadline {
                let started = Instant::now();
                let (status, _, _) = connection.request(&addr, &method, &path, &headers, &body).await;
                latencies.push(started.elapsed());
                if status != 200 {
                    failures += 1;
                }
            }
            (latencies, failures)
        }));
    }
    let mut latencies = Vec::new();
    let mut failures = 0;
    for task in tasks {
        let (l, f) = task.await.unwrap();
        latencies.extend(l);
        failures += f;
    }
    latencies.sort();
    println!(
        "{label:<28} {workers:>3} conns | {:>9.0} req/s | p50 {:>8.2?} | p99 {:>8.2?} | max {:>8.2?} | non-200: {failures}",
        latencies.len() as f64 / seconds as f64,
        percentile(&latencies, 0.50),
        percentile(&latencies, 0.99),
        latencies.last().copied().unwrap_or_default(),
    );
}

#[tokio::main]
async fn main() {
    let addr = std::env::args().nth(1).unwrap_or_else(|| "127.0.0.1:8080".to_string());
    let seconds: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(5);

    let email = format!("bench{}@example.com", std::process::id());
    let register = format!(r#"{{"email":"{email}","password":"correct horse battery","confirmPassword":"correct horse battery"}}"#);
    let mut connection = Connection::open(&addr).await;
    let (status, head, _) = connection
        .request(&addr, "POST", "/api/auth/register", "Content-Type: application/json\r\n", register.as_bytes())
        .await;
    assert_eq!(status, 201, "registering failed (is the registration rate limit hit?)");
    let cookie = head
        .lines()
        .find_map(|l| l.to_ascii_lowercase().starts_with("set-cookie:").then(|| l.to_string()))
        .and_then(|l| l.split("dc_session=").nth(1).map(|v| v.split(';').next().unwrap_or("").to_string()))
        .expect("a session cookie");

    let packet = form_urlencoded::Serializer::new(String::new())
        .append_pair("data", r#"sig;{"commands":[{"cmd":"complete_tutorial","args":[1],"number":1},{"cmd":"ping","args":[],"number":2}]}"#)
        .finish()
        .into_bytes();

    println!("target {addr}, {seconds}s per scenario");
    for workers in [1, 16, 64] {
        run(addr.clone(), cookie.clone(), workers, seconds, "packet.php (2 commands)", "POST", "/static/dragoncity/srv/packet.php", packet.clone()).await;
    }
    for workers in [1, 16, 64] {
        run(addr.clone(), cookie.clone(), workers, seconds, "get_player_info", "GET", "/static/dragoncity/srv/get_player_info.json", Vec::new()).await;
    }
    for workers in [1, 64] {
        run(addr.clone(), cookie.clone(), workers, seconds, "get_game_config (cached)", "GET", "/static/dragoncity/srv/get_game_config.json", Vec::new()).await;
    }
}
