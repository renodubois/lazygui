//! Owned disposable loopback HTTP fixture. Never calls an external account.
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

pub(crate) struct Server {
    pub(crate) url: String,
    pub(crate) request: Arc<Mutex<String>>,
    thread: Option<JoinHandle<()>>,
}
impl Server {
    pub(crate) fn respond(status: &str, body: &str, headers: &str) -> Self {
        Self::respond_after(status, body, headers, std::time::Duration::ZERO)
    }
    pub(crate) fn respond_after(
        status: &str,
        body: &str,
        headers: &str,
        delay: std::time::Duration,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/records", listener.local_addr().unwrap());
        let request = Arc::new(Mutex::new(String::new()));
        let captured = request.clone();
        let status = status.to_string();
        let body = body.to_string();
        let headers = headers.to_string();
        let thread = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                            .unwrap();
                        let mut bytes = [0; 8192];
                        let mut message = Vec::new();
                        while !message.ends_with(b"\r\n\r\n") {
                            let Ok(count) = stream.read(&mut bytes) else {
                                return;
                            };
                            if count == 0 {
                                return;
                            }
                            message.extend_from_slice(&bytes[..count]);
                        }
                        *captured.lock().unwrap() = String::from_utf8(message).unwrap();
                        std::thread::sleep(delay);
                        let reply = format!(
                            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
                            body.len()
                        );
                        let _ = stream.write_all(reply.as_bytes());
                        return;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(error) => panic!("{error}"),
                }
            }
        });
        Self {
            url,
            request,
            thread: Some(thread),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}
