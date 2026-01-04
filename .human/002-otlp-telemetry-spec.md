# OTLP Telemetry Implementation Specification

## Overview

This specification outlines the implementation of a lightweight OTLP (OpenTelemetry Protocol) telemetry system for the Trix CLI. The system is designed to track the first user-triggered command in each CLI session with immediate HTTP sending and minimal performance impact.

## Requirements

### Core Requirements
- ✅ **Single span per session** - Track only the first tracked command per CLI session
- ✅ **Immediate sending** - Direct HTTP calls with no background processing
- ✅ **Session state management** - Prevent multiple telemetry spans in one session
- ✅ **Lightweight approach** - Custom OTLP HTTP client (avoid heavy OTLP SDK)
- ✅ **Best-effort with timeout** - Silent failure on timeout
- ✅ **Global telemetry handle** - Single global instance with session state
- ✅ **Macro-based command wrapping** - Declarative telemetry instrumentation
- ✅ **Simple error code field** - String field in payload (no custom error types)
- ✅ **Anonymous user fingerprint** - Unique but privacy-preserving user ID

### Performance Requirements
- **Zero overhead when disabled** - No telemetry code runs if disabled
- **Minimal overhead when enabled** - Span creation + immediate HTTP call only
- **No shutdown complexity** - No background tasks to manage
- **Silent failures** - Network/timeout issues don't affect CLI
- **No memory overhead** - No queues or span collections maintained

## Architecture

### Module Structure
```
src/
├── telemetry/
│   ├── mod.rs              # Global handle with session state
│   ├── client.rs           # Simple OTLP HTTP client (single span only)
│   ├── span.rs             # Span data structures
│   ├── macros.rs           # Telemetry macros
│   └── fingerprint.rs      # User fingerprinting
```

### Data Flow
1. **Command Execution** - Wrapped with `with_telemetry!` macro
2. **Session Check** - Global handle checks if this is first tracked command
3. **Span Creation** - Command start/end recorded with global handle
4. **Immediate Sending** - Span sent directly to OTLP endpoint via HTTP
5. **Session Marked** - Global handle marks session as having sent telemetry

## Implementation Details

### 1. Dependencies

Add to `Cargo.toml`:
```toml
[dependencies]
# For OTLP protobuf encoding
prost = "0.13"
# For async HTTP requests (already have tokio)
# For HTTP requests (already have reqwest)
```

### 2. Configuration

#### Global Config Extension (`src/global.rs`)
```rust
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub otlp_endpoint: Option<String>,
    pub timeout_ms: u64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            otlp_endpoint: None,
            timeout_ms: 2000,
        }
    }
}
```

#### Configuration File (`~/.tx3/trix/config.toml`)
```toml
[telemetry]
enabled = true
otlp_endpoint = "https://otel.example.com:4318/v1/traces"
timeout_ms = 2000
```

### 3. Global Telemetry Handle

#### Global Handle (`src/telemetry/mod.rs`)
```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceCell;
use crate::global::TelemetryConfig;

pub static TELEMETRY_HANDLE: OnceCell<TelemetryHandle> = OnceCell::new();

pub fn init_global_telemetry(config: &TelemetryConfig) -> miette::Result<()> {
    let handle = TelemetryHandle::new(config)?;
    TELEMETRY_HANDLE.set(handle)
        .map_err(|_| miette::miette!("Telemetry already initialized"))?;
    Ok(())
}

pub fn get_global_telemetry() -> Option<&'static TelemetryHandle> {
    TELEMETRY_HANDLE.get()
}

pub fn try_record_command_execution(
    handle: &TelemetryHandle,
    command_name: &str,
    args: &dyn std::fmt::Debug,
    user_fingerprint: &str,
    body: impl FnOnce() -> Result<(), miette::Error>,
) -> Result<(), miette::Error> {
    // Check if telemetry was already sent this session
    if handle.session_sent.load(Ordering::Relaxed) {
        return body(); // Skip telemetry, just execute command
    }

    // Create and execute with telemetry
    let mut span = CommandSpan::new(command_name, args, user_fingerprint);
    let result = body();
    
    // Mark session as sent before attempting HTTP
    handle.session_sent.store(true, Ordering::Relaxed);
    
    // Complete span with result
    match &result {
        Ok(_) => span.complete(true, None),
        Err(e) => {
            let error_code = extract_simple_error_code(e);
            span.complete(false, Some(error_code));
        }
    }

    // Send telemetry asynchronously (fire and forget)
    if let Some(ref client) = handle.client {
        tokio::spawn(async move {
            let _ = client.send_span(span).await; // Silent failure
        });
    }

    result
}

pub struct TelemetryHandle {
    client: Option<OtlpClient>,
    user_fingerprint: String,
    session_sent: AtomicBool,
}

impl TelemetryHandle {
    pub fn new(config: &TelemetryConfig) -> miette::Result<Self> {
        let client = config.otlp_endpoint.as_ref()
            .map(|endpoint| OtlpClient::new(endpoint.clone(), config.timeout_ms));
        
        let user_fingerprint = get_user_fingerprint().to_string();
        
        Ok(Self {
            client,
            user_fingerprint,
            session_sent: AtomicBool::new(false),
        })
    }
    
    pub fn get_user_fingerprint(&self) -> &str {
        &self.user_fingerprint
    }
}
```

### 4. Span Data Structures

#### Command Span (`src/telemetry/span.rs`)
```rust
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct CommandSpan {
    pub command_name: String,
    pub args_hash: String,
    pub start_time: SystemTime,
    pub end_time: Option<SystemTime>,
    pub success: Option<bool>,
    pub error_code: Option<String>,
    pub trix_version: String,
    pub profile: Option<String>,
    pub user_fingerprint: String,
}

impl CommandSpan {
    pub fn new(command_name: &str, args: &dyn std::fmt::Debug, user_fingerprint: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        format!("{:?}", args).hash(&mut hasher);

        Self {
            command_name: command_name.to_string(),
            args_hash: format!("{:x}", hasher.finish()),
            start_time: SystemTime::now(),
            end_time: None,
            success: None,
            error_code: None,
            trix_version: env!("CARGO_PKG_VERSION").to_string(),
            profile: None,
            user_fingerprint: user_fingerprint.to_string(),
        }
    }

    pub fn complete(&mut self, success: bool, error_code: Option<String>) {
        self.end_time = Some(SystemTime::now());
        self.success = Some(success);
        self.error_code = error_code;
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

    pub fn set_profile(&mut self, profile: &str) {
        self.profile = Some(profile.to_string());
    }
}
```

### 5. Anonymous User Fingerprinting

#### Fingerprint Generation (`src/telemetry/fingerprint.rs`)
```rust
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::OnceCell;

static USER_FINGERPRINT: OnceCell<String> = OnceCell::new();

pub fn get_user_fingerprint() -> &'static str {
    USER_FINGERPRINT.get_or_init(|| {
        // Try to load from persistent storage first
        if let Some(stored) = load_stored_fingerprint() {
            stored
        } else {
            // Generate new one and store it
            let fingerprint = generate_user_fingerprint();
            store_fingerprint(&fingerprint);
            fingerprint
        }
    })
}

fn generate_user_fingerprint() -> String {
    // Generate a stable but anonymous fingerprint
    let mut hasher = DefaultHasher::new();
    
    // Use machine-specific but non-identifying data
    if let Some(hostname) = get_hostname() {
        hostname.hash(&mut hasher);
    }
    
    // Use OS-specific non-identifying data
    if let Some(os_info) = get_os_info() {
        os_info.hash(&mut hasher);
    }
    
    // Add some randomness for uniqueness
    let random_component = generate_stable_random();
    random_component.hash(&mut hasher);
    
    // Format as hex string
    format!("{:x}", hasher.finish())
}

fn load_stored_fingerprint() -> Option<String> {
    // Load from ~/.tx3/trix/fingerprint
    let mut path = crate::home::tx3_dir().ok()?;
    path.push("trix/fingerprint");
    std::fs::read_to_string(path).ok()
}

fn store_fingerprint(fingerprint: &str) {
    // Store in ~/.tx3/trix/fingerprint
    if let Ok(mut path) = crate::home::tx3_dir() {
        path.push("trix");
        if std::fs::create_dir_all(&path).is_ok() {
            path.push("fingerprint");
            let _ = std::fs::write(path, fingerprint);
        }
    }
}
```

#### Fingerprint Properties
- **Stable**: Same across CLI restarts
- **Anonymous**: No personal information
- **Unique**: Low collision probability
- **Optional**: Users can reset by deleting fingerprint file
- **Privacy-compliant**: GDPR-friendly approach

### 6. Telemetry Macros

#### Sync Command Macro (`src/telemetry/macros.rs`)
```rust
#[macro_export]
macro_rules! with_telemetry {
    (
        $command_name:literal,
        $args:ident,
        $config:ident,
        $profile:ident,
        $body:block
    ) => {{
        use $crate::telemetry::{
            get_global_telemetry, try_record_command_execution,
            get_user_fingerprint
        };
        
        match get_global_telemetry() {
            Some(telemetry) => try_record_command_execution(
                telemetry,
                $command_name,
                &$args,
                telemetry.get_user_fingerprint(),
                || $body
            ),
            None => $body, // Telemetry not initialized
        }
    }};
}
```

#### Async Command Macro (`src/telemetry/macros.rs`)
```rust
#[macro_export]
macro_rules! with_telemetry_async {
    (
        $command_name:literal,
        $args:ident,
        $config:ident,
        $profile:ident,
        $body:block
    ) => {{
        use $crate::telemetry::{
            get_global_telemetry, try_record_command_execution,
            get_user_fingerprint
        };
        
        match get_global_telemetry() {
            Some(telemetry) => try_record_command_execution(
                telemetry,
                $command_name,
                &$args,
                telemetry.get_user_fingerprint(),
                || (async move $body).into()
            ),
            None => (async move $body).into(),
        }
    }};
}
```

#### Simple Error Code Extraction
```rust
pub fn extract_simple_error_code(error: &miette::Error) -> String {
    let error_msg = error.to_string().to_lowercase();
    
    // Simple pattern matching for common error categories
    if error_msg.contains("network") || error_msg.contains("connection") {
        "network_error".to_string()
    } else if error_msg.contains("file") || error_msg.contains("directory") {
        "filesystem_error".to_string()
    } else if error_msg.contains("parse") || error_msg.contains("syntax") {
        "parse_error".to_string()
    } else if error_msg.contains("template") {
        "template_error".to_string()
    } else if error_msg.contains("transaction") {
        "transaction_error".to_string()
    } else if error_msg.contains("validation") {
        "validation_error".to_string()
    } else {
        "unknown_error".to_string()
    }
}
```

### 7. Simple OTLP HTTP Client

#### Lightweight Client (`src/telemetry/client.rs`)
```rust
use reqwest::Client;
use serde_json::json;
use std::time::Duration;

use crate::telemetry::span::CommandSpan;

#[derive(Clone)]
pub struct OtlpClient {
    client: Client,
    endpoint: String,
    timeout: Duration,
}

impl OtlpClient {
    pub fn new(endpoint: String, timeout_ms: u64) -> Self {
        Self {
            client: Client::new(),
            endpoint,
            timeout: Duration::from_millis(timeout_ms),
        }
    }
    
    pub async fn send_span(&self, span: CommandSpan) -> Result<(), ()> {
        let payload = self.encode_span(span);
        
        match tokio::time::timeout(self.timeout, self.client.post(&self.endpoint)
            .json(&payload)
            .send()
        ).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(_)) | Err(_) => Err(()),  // Silent failure
        }
    }
    
    fn encode_span(&self, span: CommandSpan) -> serde_json::Value {
        // Manual OTLP JSON encoding for single span
        json!({
            "resource": {
                "service.name": "trix-cli",
                "service.version": span.trix_version,
                "user.fingerprint": span.user_fingerprint
            },
            "spans": [{
                "name": span.command_name,
                "attributes": {
                    "command.name": span.command_name,
                    "args.hash": span.args_hash,
                    "success": span.success,
                    "error.code": span.error_code,
                    "duration.ms": span.duration_ms(),
                    "profile": span.profile,
                    "user.fingerprint": span.user_fingerprint
                }
            }]
        })
    }
}
```

## Command Integration

### Tracked Commands (Hardcoded)

```rust
// In telemetry/mod.rs
pub const TRACKED_COMMANDS: &[&str] = &[
    "invoke",    // Transaction invocation
    "codegen",   // Code generation
    "build",     // Build Tx3 files
    "test",      // Run tests
    "check",     // Check packages
    "devnet",    // Start devnet
];

pub fn is_tracked_command(command_name: &str) -> bool {
    TRACKED_COMMANDS.contains(&command_name)
}
```

### Command Integration Pattern

#### Sync Commands (`src/commands/invoke.rs`)
```rust
pub fn run(args: Args, config: &RootConfig, profile: &ProfileConfig) -> miette::Result<()> {
    crate::with_telemetry!("invoke", args, config, profile, {
        // Existing invoke logic unchanged
        invoke_logic(args, config, profile)
    })
}
```

#### Async Commands (`src/commands/codegen.rs`)
```rust
pub async fn run(args: Args, config: &RootConfig, profile: &ProfileConfig) -> miette::Result<()> {
    crate::with_telemetry_async!("codegen", args, config, profile, {
        // Existing codegen logic unchanged
        codegen_logic(args, config, profile).await
    })
}
```

### Commands to Modify

Only these commands need the macro wrapper:

1. **`invoke`** - Transaction invocation
2. **`codegen`** - Code generation  
3. **`build`** - Build Tx3 files
4. **`test`** - Run tests
5. **`check`** - Check packages
6. **`devnet`** - Start devnet

### Main.rs Integration

#### Global Initialization (`src/main.rs`)
```rust
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config()?;
    global::ensure_global_config()?;
    
    let global_config = global::read_config()?;
    
    // Initialize global telemetry (non-blocking)
    if global_config.telemetry.enabled {
        telemetry::init_global_telemetry(&global_config.telemetry)?;
    }
    
    // Run commands with existing logic
    let result = match config {
        Some(config) => run_scoped_command(cli, config).await,
        None => run_global_command(cli),
    };
    
    // No complex shutdown needed - telemetry is fire-and-forget
    result
}
```

## OTLP Payload Structure

### JSON Format
```json
{
  "resource": {
    "service.name": "trix-cli",
    "service.version": "0.19.3",
    "user.fingerprint": "a1b2c3d4e5f6..."
  },
  "spans": [
    {
      "name": "invoke",
      "attributes": {
        "command.name": "invoke",
        "args.hash": "deadbeef...",
        "success": true,
        "error.code": null,
        "duration.ms": 1234,
        "profile": "local",
        "user.fingerprint": "a1b2c3d4e5f6..."
      }
    }
  ]
}
```

### Error Code Categories
- `network_error` - Network/connection issues
- `filesystem_error` - File/directory access issues
- `parse_error` - Syntax/parsing issues
- `template_error` - Template-related issues
- `transaction_error` - Transaction execution issues
- `validation_error` - Input validation issues
- `unknown_error` - Unclassified errors

## Privacy Considerations

### Data Collection Principles
- **Anonymous**: No personal identifiers or IP addresses
- **Minimal**: Only command metadata and execution metrics
- **Hashed**: Command arguments are hashed for privacy
- **Opt-out**: Users can disable telemetry with `trix telemetry off`
- **Transparent**: Clear documentation of what data is collected

### Fingerprint Privacy
- **Machine-specific**: Uses hostname, OS version, architecture
- **Stable random**: Generated once and stored persistently
- **No personal data**: No usernames, emails, or identifying information
- **Resettable**: Users can delete fingerprint file to reset

## Testing Strategy

### Unit Tests
- OTLP encoding and span creation
- Error code extraction logic
- Fingerprint generation and persistence
- Session state management
- Configuration loading and validation

### Integration Tests
- End-to-end telemetry flow with mock OTLP endpoint
- Session state prevents multiple spans
- Silent failure behavior
- Macro behavior with/without telemetry enabled

### Performance Tests
- Ensure <10ms overhead for telemetry setup
- CLI performance impact measurement
- Memory usage verification (no span accumulation)

### Session Management Tests
- Verify only first command triggers telemetry
- Test subsequent commands are skipped
- Validate session state persistence during process lifetime

## Deployment Considerations

### Feature Flags
- Optional `telemetry` feature for builds
- Default disabled for privacy compliance

### Default Configuration
- Ship with telemetry disabled by default
- Clear opt-in process for users
- Easy opt-out with `trix telemetry off`

### Documentation
- Clear explanation of data collection
- Privacy policy and user rights
- Configuration options and customization

## Implementation Phases

### Phase 1: Core Infrastructure
- [ ] Create simplified telemetry module structure
- [ ] Implement global handle with session state
- [ ] Build simple OTLP HTTP client

### Phase 2: Data Collection
- [ ] Implement span data structures
- [ ] Add anonymous fingerprinting
- [ ] Create telemetry macros with session checking

### Phase 3: Command Integration
- [ ] Wrap tracked commands with macros
- [ ] Update main.rs for global initialization
- [ ] Test end-to-end telemetry flow

### Phase 4: Testing & Validation
- [ ] Add comprehensive test suite
- [ ] Performance testing and optimization
- [ ] Privacy validation and documentation

## Success Criteria

### Functional Requirements
- ✅ Exactly one telemetry span sent per session
- ✅ Only first tracked command triggers telemetry
- ✅ Immediate HTTP sending with no background processing
- ✅ Silent failure behavior maintains CLI performance

### Performance Requirements
- ✅ <10ms overhead for telemetry initialization
- ✅ <1ms overhead for session checking
- ✅ No memory overhead beyond single span
- ✅ CLI performance unaffected when telemetry disabled

### Privacy Requirements
- ✅ No personal data collected
- ✅ Anonymous fingerprinting implemented
- ✅ User control over telemetry enablement
- ✅ Transparent data collection documentation

## Maintenance

### Monitoring
- Track telemetry delivery success rates
- Monitor HTTP request performance
- Watch for error patterns

### Updates
- Update error code patterns for new error types
- Maintain compatibility with OTLP endpoint changes
- Add new commands to tracked list as needed

### Support
- Provide troubleshooting guide for telemetry issues
- Document common configuration problems
- Support user inquiries about data privacy

---

**Specification Version**: 2.0  
**Last Updated**: 2025-01-03  
**Status**: Ready for Implementation