use reqwest::Client;
use serde_json::json;
use std::time::Duration;

use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct CommandSpan {
    pub command_name: String,
    pub start_time: SystemTime,
    pub end_time: Option<SystemTime>,
    pub success: Option<bool>,
    pub error: Option<String>,
}

impl CommandSpan {
    pub fn new(command_name: &str) -> Self {
        Self {
            command_name: command_name.to_string(),
            start_time: SystemTime::now(),
            end_time: None,
            success: None,
            error: None,
        }
    }

    pub fn complete(&mut self, success: bool, error: Option<String>) {
        self.end_time = Some(SystemTime::now());
        self.success = Some(success);
        self.error = error;
    }

    pub fn duration_ms(&self) -> Option<u64> {
        match self.end_time {
            Some(end_time) => end_time
                .duration_since(self.start_time)
                .ok()
                .map(|d| d.as_millis() as u64),
            None => None,
        }
    }
}

#[derive(Clone)]
pub struct OtlpClient {
    client: Client,
    endpoint: String,
    timeout: Duration,
    user_fingerprint: String,
}

impl OtlpClient {
    pub fn new(endpoint: String, timeout_ms: u64, user_fingerprint: String) -> Self {
        Self {
            client: Client::new(),
            endpoint,
            timeout: Duration::from_millis(timeout_ms),
            user_fingerprint,
        }
    }

    pub async fn send_span(&self, span: CommandSpan) -> Result<(), ()> {
        let payload = self.encode_span(span);

        match tokio::time::timeout(
            self.timeout,
            self.client.post(&self.endpoint).json(&payload).send(),
        )
        .await
        {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(_)) | Err(_) => Err(()), // Silent failure
        }
    }

    fn encode_span(&self, span: CommandSpan) -> serde_json::Value {
        // Manual OTLP JSON encoding for single span
        json!({
            "resource": {
                "service.name": "trix-cli",
                "service.version": env!("CARGO_PKG_VERSION"),
                "user.fingerprint": self.user_fingerprint
            },
            "spans": [{
                "name": span.command_name,
                "attributes": {
                    "command.name": span.command_name,
                    "success": span.success,
                    "error": span.error,
                    "duration.ms": span.duration_ms(),
                }
            }]
        })
    }
}
