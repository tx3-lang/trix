use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use miette::IntoDiagnostic;

use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;

pub const DEFAULT_TELEMETRY_ENDPOINT: &str = "http://localhost:4318/v1/metrics";

pub fn init_telemetry(endpoint: Option<String>) -> miette::Result<opentelemetry_sdk::metrics::SdkMeterProvider> {
    let otlp_endpoint = endpoint.unwrap_or_else(|| DEFAULT_TELEMETRY_ENDPOINT.to_string());
     // Initialize OTLP exporter using HTTP binary protocol
    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_http()
        .with_protocol(opentelemetry_otlp::Protocol::HttpBinary)
        .with_endpoint(otlp_endpoint)
        .build().into_diagnostic()?;

    let resource = Resource::builder()
        .with_attribute(KeyValue::new("service.name", "trix"))
        .with_attribute(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")))
        .build();

    // Create a meter provider with the OTLP Metric exporter
    let meter_provider = opentelemetry_sdk::metrics::SdkMeterProvider::builder()
        .with_periodic_exporter(exporter)
        .with_resource(resource)
        .build();
    global::set_meter_provider(meter_provider.clone());

    Ok(meter_provider)
}

/// Report command execution metrics to OpenTelemetry
async fn report_command_execution(command: &str, status: &str) -> miette::Result<()> {
    let user_fingerprint = get_user_fingerprint()?;

    // Send command execution metric
    send_command_metric(command, &user_fingerprint, status).await?;

    Ok(())
}

/// Send command metrics to OpenTelemetry Collector in OTLP format
async fn send_command_metric(command: &str, user_fingerprint: &str, status: &str) -> miette::Result<()> {
    let meter = global::meter("trix");

    let exec_cmd_counter = meter
        .u64_counter("command_executed")
        .with_description("Counts the number of executed commands")
        .build();

    exec_cmd_counter.add(1, &vec![
        KeyValue::new("command", command.to_string()),
        KeyValue::new("user_fingerprint", user_fingerprint.to_string()),
        KeyValue::new("status", status.to_string()),
        KeyValue::new("version", env!("CARGO_PKG_VERSION").to_string()),
    ]);

    Ok(())
}

pub fn report_command_result(command: &str, success: &bool) {
    if let Ok(config) = crate::global::read_config() {
        if config.telemetry.enabled {
            let command_owned = command.to_string();
            let status = if *success { "success" } else { "error" };
            send_telemetry_blocking(&command_owned, &status);
        }
    }
}

/// Send telemetry in a blocking manner, handling runtime context appropriately
fn send_telemetry_blocking(command: &str, status: &str) {
    futures::executor::block_on(async {
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            report_command_execution(&command, &status)
        ).await
    }).ok();
}

/// Generates an anonymous user fingerprint based on system information
/// This function prioritizes anonymity by using hardware-specific identifiers
/// rather than user-identifiable information like usernames or hostnames
pub fn generate_user_fingerprint() -> miette::Result<String> {
    let mut hasher = DefaultHasher::new();
    let mut entropy_sources = 0;
    
    // Add a salt to prevent rainbow table attacks
    "trix-anonymous-user-fingerprint-v1".hash(&mut hasher);
    
    // Platform-specific hardware identifiers (most anonymous)
    #[cfg(target_os = "macos")]
    {
        // Try to get hardware UUID (hardware-specific, not user-specific)
        if let Ok(output) = std::process::Command::new("system_profiler")
            .arg("SPHardwareDataType")
            .output()
        {
            if let Ok(hardware_info) = String::from_utf8(output.stdout) {
                if let Some(uuid_line) = hardware_info
                    .lines()
                    .find(|line| line.contains("Hardware UUID"))
                {
                    uuid_line.hash(&mut hasher);
                    entropy_sources += 1;
                }
            }
        }
        
        // Try to get system serial number
        if let Ok(output) = std::process::Command::new("system_profiler")
            .arg("SPHardwareDataType")
            .output()
        {
            if let Ok(hardware_info) = String::from_utf8(output.stdout) {
                if let Some(serial_line) = hardware_info
                    .lines()
                    .find(|line| line.contains("Serial Number"))
                {
                    serial_line.hash(&mut hasher);
                    entropy_sources += 1;
                }
            }
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        // Machine ID is hardware/installation specific, not user specific
        if let Ok(machine_id) = std::fs::read_to_string("/etc/machine-id")
            .or_else(|_| std::fs::read_to_string("/var/lib/dbus/machine-id"))
        {
            machine_id.trim().hash(&mut hasher);
            entropy_sources += 1;
        }
        
        // Try to get DMI product UUID
        if let Ok(product_uuid) = std::fs::read_to_string("/sys/class/dmi/id/product_uuid") {
            product_uuid.trim().hash(&mut hasher);
            entropy_sources += 1;
        }
        
        // CPU info can provide hardware-specific entropy
        if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
            // Extract only hardware-specific lines, avoiding frequencies that may vary
            for line in cpuinfo.lines() {
                if line.starts_with("processor") || 
                   line.starts_with("vendor_id") ||
                   line.starts_with("cpu family") ||
                   line.starts_with("model") ||
                   line.starts_with("microcode") {
                    line.hash(&mut hasher);
                }
            }
            entropy_sources += 1;
        }
    }
    
    #[cfg(target_os = "windows")]
    {
        // Windows hardware UUID
        if let Ok(output) = std::process::Command::new("wmic")
            .args(["csproduct", "get", "UUID", "/value"])
            .output()
        {
            if let Ok(uuid_info) = String::from_utf8(output.stdout) {
                uuid_info.hash(&mut hasher);
                entropy_sources += 1;
            }
        }
        
        // Motherboard serial number
        if let Ok(output) = std::process::Command::new("wmic")
            .args(["baseboard", "get", "SerialNumber", "/value"])
            .output()
        {
            if let Ok(serial_info) = String::from_utf8(output.stdout) {
                serial_info.hash(&mut hasher);
                entropy_sources += 1;
            }
        }
    }
    
    // If we couldn't get enough hardware-specific entropy, use filesystem info
    // This is less ideal but still reasonably anonymous
    if entropy_sources < 2 {
        // Get filesystem creation time or similar system-specific info
        if let Ok(metadata) = std::fs::metadata("/") {
            if let Ok(created) = metadata.created() {
                created.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default().as_secs().hash(&mut hasher);
                entropy_sources += 1;
            }
        }
        
        // Add some environment entropy that's less identifying
        let env_entropy: Vec<String> = std::env::vars()
            .filter_map(|(key, value)| {
                // Only use system-level env vars, avoid user-specific ones
                if key.starts_with("LC_") || 
                   key == "LANG" || 
                   key == "PATH" ||
                   key == "SHELL" ||
                   key.starts_with("XDG_") {
                    Some(format!("{}={}", key, value))
                } else {
                    None
                }
            })
            .collect();
        
        for env_var in env_entropy {
            env_var.hash(&mut hasher);
        }
        entropy_sources += 1;
    }
    
    // Final fallback: generate a random component and store it persistently
    // This ensures we always have a stable identifier even if hardware info is unavailable
    if entropy_sources == 0 {
        return Err(miette::miette!("Unable to generate anonymous fingerprint: insufficient entropy sources"));
    }
    
    // Generate final hash using Rust's standard hasher
    let result = hasher.finish();
    
    // Convert to hex string for readability
    Ok(format!("{:016x}", result))
}

/// Ensures that the telemetry configuration has a user fingerprint
pub fn ensure_user_fingerprint() -> miette::Result<String> {
    let mut config = crate::global::read_config()?;
    
    if let Some(ref user_id) = config.telemetry.user_fingerprint {
        Ok(user_id.clone())
    } else {
        let user_id = generate_user_fingerprint()?;
        config.telemetry.user_fingerprint = Some(user_id.clone());
        crate::global::save_config(&config)?;
        Ok(user_id)
    }
}

/// Gets the current user fingerprint, generating one if it doesn't exist
pub fn get_user_fingerprint() -> miette::Result<String> {
    ensure_user_fingerprint()
}
