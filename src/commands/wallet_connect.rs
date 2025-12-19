use crate::config::RootConfig;
use clap::Args as ClapArgs;
use miette::IntoDiagnostic;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const HTML_TEMPLATE: &str = include_str!("../../templates/wallet_connect.html");

#[derive(ClapArgs)]
pub struct Args {
    /// Port to serve wallet connect on.
    #[arg(long, default_value_t = 3030)]
    port: u16,

    /// Optional CBOR hex to pre-fill
    #[arg(long)]
    cbor: Option<String>,
}

pub async fn run(args: Args, _config: &RootConfig) -> miette::Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    let listener = TcpListener::bind(addr).await.into_diagnostic()?;

    println!("Serving wallet connect on port {}", args.port);
    println!("http://localhost:{}", args.port);

    let cbor_value = args.cbor.unwrap_or_default();
    let body = HTML_TEMPLATE.replace("__CBOR_PLACEHOLDER__", &cbor_value);
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );

    loop {
        let (mut socket, _) = listener.accept().await.into_diagnostic()?;
        let response = response.clone();

        tokio::spawn(async move {
            let mut buf = [0; 1024];
            // Leemos la solicitud (aunque la ignoramos para este ejemplo simple)
            let _ = socket.read(&mut buf).await;

            if let Err(e) = socket.write_all(response.as_bytes()).await {
                eprintln!("failed to write to socket; err = {:?}", e);
            }
        });
    }
}
