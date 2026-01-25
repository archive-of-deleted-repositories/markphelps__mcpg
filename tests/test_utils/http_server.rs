use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

/// Find an available port starting from a base port
fn find_available_port(start_port: u16) -> Option<u16> {
    for port in start_port..start_port + 100 {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Some(port);
        }
    }
    None
}

/// Helper to spawn example-remote-server for integration tests
pub struct TestHttpServer {
    process: Option<Child>,
    pub url: String,
    port: u16,
}

impl TestHttpServer {
    /// Start the example-remote-server
    ///
    /// Returns None if server cannot be started (npm/node not available)
    pub fn start() -> Option<Self> {
        // Check if we should skip HTTP tests
        if std::env::var("SKIP_HTTP_TESTS").is_ok() {
            return None;
        }

        // Find an available port
        let port = find_available_port(3232)?;

        // Try to start the server
        let child = Command::new("npx")
            .args(&["-y", "@modelcontextprotocol/server-everything"])
            .env("PORT", port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let server = TestHttpServer {
            process: Some(child),
            url: format!("http://localhost:{}/mcp", port),
            port,
        };

        // Wait for server to be ready with polling
        if !server.wait_for_ready(Duration::from_secs(5)) {
            return None;
        }

        Some(server)
    }

    /// Check if server is responding
    fn is_ready(&self) -> bool {
        // Try to connect to the port
        std::net::TcpStream::connect(("127.0.0.1", self.port))
            .and_then(|stream| {
                stream.set_read_timeout(Some(Duration::from_secs(1)))?;
                Ok(())
            })
            .is_ok()
    }

    /// Wait for server to be ready with polling
    fn wait_for_ready(&self, timeout: Duration) -> bool {
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(100);

        while start.elapsed() < timeout {
            if self.is_ready() {
                return true;
            }
            thread::sleep(poll_interval);
        }
        false
    }
}

impl Drop for TestHttpServer {
    fn drop(&mut self) {
        if let Some(mut child) = self.process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
