//! Minimal in-process OCI Distribution registry stub for the offline suites.
//!
//! Serves a pre-built set of routes over plain HTTP on `127.0.0.1:<random>`,
//! which is exactly what `oci::client_for` speaks when the configured
//! registry URL starts with `http://`. Only the read side of the pull flow
//! is implemented — the three requests an anonymous `oci_client::Client::pull`
//! makes:
//!
//! 1. `GET /v2/`               → 200 with no `WWW-Authenticate` (anonymous ok)
//! 2. `GET /v2/<repo>/manifests/<tag>` → OCI image manifest JSON
//! 3. `GET /v2/<repo>/blobs/<digest>`  → config / layer bytes
//!
//! Digests are real sha256 values because `oci_client` verifies every blob
//! against its descriptor and hashes the manifest body itself when no
//! `Docker-Content-Digest` header is present.
//!
//! Every request path is recorded so tests can assert *which* repository
//! path the client addressed (e.g. the lowercased form of an uppercase
//! scope). Unknown paths get a spec-shaped 404, which surfaces in `trix` as
//! a pull failure.

#![allow(dead_code)] // only the CLI suite pulls; the contract suite does not

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use sha2::{Digest as _, Sha256};

pub const OCI_MANIFEST_MEDIA_TYPE: &str = "application/vnd.oci.image.manifest.v1+json";
pub const OCI_CONFIG_MEDIA_TYPE: &str = "application/vnd.oci.image.config.v1+json";

pub fn sha256_digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

pub struct StubRoute {
    pub content_type: String,
    pub body: Vec<u8>,
}

/// A published protocol image as the stub serves it. `repo` is the exact
/// repository path the manifest and blobs are registered under — tests that
/// exercise case mapping serve only the lowercase path and let the recorded
/// requests prove what the client asked for.
pub struct StubProtocolImage {
    pub repo: String,
    pub tag: String,
    /// JSON in the shape of `trix::interfaces::oci::ImageMetadata`.
    pub metadata: serde_json::Value,
    /// `(media_type, bytes)` per layer, in manifest order.
    pub layers: Vec<(String, Vec<u8>)>,
}

impl StubProtocolImage {
    pub fn routes(&self) -> HashMap<String, StubRoute> {
        let mut routes = HashMap::new();

        routes.insert(
            "/v2/".to_string(),
            StubRoute {
                content_type: "application/json".to_string(),
                body: b"{}".to_vec(),
            },
        );

        let config_bytes = serde_json::to_vec(&self.metadata).expect("serialize stub metadata");
        let config_digest = sha256_digest(&config_bytes);

        let mut layer_descriptors = Vec::new();
        for (media_type, bytes) in &self.layers {
            let digest = sha256_digest(bytes);
            layer_descriptors.push(serde_json::json!({
                "mediaType": media_type,
                "digest": digest,
                "size": bytes.len(),
            }));
            routes.insert(
                format!("/v2/{}/blobs/{}", self.repo, digest),
                StubRoute {
                    content_type: "application/octet-stream".to_string(),
                    body: bytes.clone(),
                },
            );
        }

        routes.insert(
            format!("/v2/{}/blobs/{}", self.repo, config_digest),
            StubRoute {
                content_type: OCI_CONFIG_MEDIA_TYPE.to_string(),
                body: config_bytes.clone(),
            },
        );

        let manifest = serde_json::json!({
            "schemaVersion": 2,
            "mediaType": OCI_MANIFEST_MEDIA_TYPE,
            "config": {
                "mediaType": OCI_CONFIG_MEDIA_TYPE,
                "digest": config_digest,
                "size": config_bytes.len(),
            },
            "layers": layer_descriptors,
        });
        routes.insert(
            format!("/v2/{}/manifests/{}", self.repo, self.tag),
            StubRoute {
                content_type: OCI_MANIFEST_MEDIA_TYPE.to_string(),
                body: serde_json::to_vec(&manifest).expect("serialize stub manifest"),
            },
        );

        routes
    }
}

pub struct OciRegistryStub {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl OciRegistryStub {
    pub fn serve(routes: HashMap<String, StubRoute>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub registry");
        let addr = listener.local_addr().expect("stub registry local addr");
        let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let routes = Arc::new(routes);
        let thread_requests = Arc::clone(&requests);
        let thread_shutdown = Arc::clone(&shutdown);

        let handle = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if thread_shutdown.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                handle_connection(stream, &routes, &thread_requests);
            }
        });

        Self {
            addr,
            requests,
            shutdown,
            handle: Some(handle),
        }
    }

    /// Registry URL in the form `trix.toml`'s `[registry].url` expects.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Every request line seen so far, as `"<METHOD> <path>"`.
    pub fn requested_paths(&self) -> Vec<String> {
        self.requests.lock().expect("stub request log").clone()
    }
}

impl Drop for OciRegistryStub {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        // Wake the accept loop so the thread observes the flag.
        let _ = TcpStream::connect(self.addr);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    routes: &HashMap<String, StubRoute>,
    requests: &Mutex<Vec<String>>,
) {
    // Read the request head; the pull flow only issues body-less GETs.
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    while !head_complete(&buf) {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => return,
        }
        if buf.len() > 64 * 1024 {
            break;
        }
    }

    let head = String::from_utf8_lossy(&buf);
    let mut request_line = head.lines().next().unwrap_or("").split_whitespace();
    let method = request_line.next().unwrap_or("");
    let path = request_line
        .next()
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("");

    requests
        .lock()
        .expect("stub request log")
        .push(format!("{method} {path}"));

    let response = match routes.get(path) {
        Some(route) => http_response(200, "OK", &route.content_type, &route.body),
        None => http_response(
            404,
            "Not Found",
            "application/json",
            br#"{"errors":[{"code":"NAME_UNKNOWN","message":"repository name not known to registry"}]}"#,
        ),
    };

    let _ = stream.write_all(&response);
    let _ = stream.flush();
}

fn head_complete(buf: &[u8]) -> bool {
    buf.windows(4).any(|w| w == b"\r\n\r\n")
}

fn http_response(status: u16, reason: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}
