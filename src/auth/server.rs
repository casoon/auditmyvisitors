/// Minimal TCP server that listens for the OAuth redirect on localhost.
/// Returns the full query string (e.g. "code=...&state=...").
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

use anyhow::{bail, Context};

pub fn wait_for_redirect(port: u16) -> anyhow::Result<String> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
        .with_context(|| format!("Cannot bind to port {port} for OAuth redirect"))?;

    // Set a 2-minute timeout so we don't hang forever
    listener
        .set_nonblocking(false)
        .context("Cannot configure listener")?;

    let (mut stream, _) = listener
        .accept()
        .context("Did not receive OAuth redirect in time")?;

    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .context("Cannot read redirect request")?;

    // Send a simple HTML response so the browser shows something useful
    let html = "<html><body><h2>Login erfolgreich ✓</h2><p>Du kannst diesen Tab schließen.</p></body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
        html.len(),
        html
    );
    stream
        .write_all(response.as_bytes())
        .context("Cannot write redirect response")?;

    // Extract query string from "GET /?code=...&state=... HTTP/1.1"
    let path = request_line
        .split_whitespace()
        .nth(1)
        .context("Malformed HTTP request from browser")?;

    let query = path
        .split_once('?')
        .map(|(_, q)| q)
        .unwrap_or("");

    if query.is_empty() {
        bail!("No query parameters received in OAuth redirect");
    }

    Ok(query.to_string())
}
