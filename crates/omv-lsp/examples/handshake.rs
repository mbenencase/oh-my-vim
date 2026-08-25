//! Manual probe: spawn a server and print every event for 20s.
//! Run with: cargo run -p omv-lsp --example handshake -- rust-analyzer
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

fn main() {
    let command = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "rust-analyzer".into());
    let root = std::env::current_dir().unwrap();
    let (tx, rx) = channel();
    let spec = omv_lsp::ServerSpec {
        command,
        args: vec![],
    };
    let _client = omv_lsp::Client::spawn("rust", &spec, &root, tx).expect("spawn");

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        if let Ok(event) = rx.recv_timeout(Duration::from_millis(500)) {
            println!("[{:>5.1}s] {event:?}", start.elapsed().as_secs_f32());
        }
    }
    println!("done");
}
