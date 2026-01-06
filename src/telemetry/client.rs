use reqwest::{
    Client,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use serde_json::json;
use std::{collections::HashMap, time::Duration};
use tracing::{debug, warn};

use crate::{global::TelemetryConfig, telemetry::fingerprint};

#[derive(Debug, Clone)]
pub struct CommandMetric {
    pub command_name: String,
}

impl CommandMetric {
    pub fn new(command_name: &str) -> Self {
        Self {
            command_name: command_name.to_string(),
        }
    }
}

// best effort parsing of headers, anything invalid is ignored
fn parse_headers(headers: HashMap<String, String>) -> HeaderMap {
    let mut parsed_headers = HeaderMap::new();

    for (key, value) in headers {
        let Ok(key) = HeaderName::try_from(key) else {
            continue;
        };

        let Ok(value) = HeaderValue::try_from(value) else {
            continue;
        };

        parsed_headers.insert(key, value);
    }

    parsed_headers
}

#[derive(Clone)]
pub struct OtlpClient {
    client: Client,
    endpoint: String,
    headers: HeaderMap,
    timeout: Duration,
    user: String,
}

impl OtlpClient {
    pub fn setup(config: &TelemetryConfig) -> Self {
        Self {
            client: Client::new(),
            endpoint: config.otlp_endpoint.clone(),
            headers: parse_headers(config.otlp_headers.clone()),
            timeout: Duration::from_millis(config.timeout_ms),
            user: fingerprint::get_user_fingerprint(),
        }
    }

    pub async fn send_metric(&self, metric: CommandMetric) -> Result<(), ()> {
        let payload = self.encode_metric(metric);

        let endpoint = format!("{}/v1/metrics", self.endpoint);

        let request = self
            .client
            .post(&endpoint)
            .json(&payload)
            .headers(self.headers.clone());

        let result = tokio::time::timeout(self.timeout, request.send()).await;

        match result {
            Ok(Ok(_)) => {
                debug!("metric sent successfully");
                Ok(())
            }
            Ok(Err(_)) | Err(_) => {
                warn!("metric sent failed");
                Err(())
            }
        }
    }

    fn encode_metric(&self, metric: CommandMetric) -> serde_json::Value {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        // Manual OTLP JSON encoding for single metric
        json!({
            "resourceMetrics": [{
                "resource": {
                    "attributes": [{
                        "key": "service.name",
                        "value": {"stringValue": "trix"}
                    }, {
                        "key": "service.version",
                        "value": {"stringValue": env!("CARGO_PKG_VERSION")}
                    }, {
                        "key": "user.fingerprint",
                        "value": {"stringValue": self.user}
                    }]
                },
                "scopeMetrics": [{
                    "scope": {},
                    "metrics": [{
                        "name": "command_invocation",
                        "sum": {
                            "dataPoints": [{
                                "attributes": [{
                                    "key": "command_name",
                                    "value": {"stringValue": metric.command_name}
                                }],
                                "timeUnixNano": format!("{}", timestamp),
                                "asInt": "1"
                            }],
                            "aggregationTemporality": 1,
                            "isMonotonic": true
                        }
                    }]
                }]
            }]
        })
    }
}
