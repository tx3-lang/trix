# Lightweight Telemetry Design

## Overview

This design outlines the implementation of a lightweight metrics telemetry system for the Trix CLI. The system is designed to track command invocation counters with immediate HTTP sending and minimal performance impact.

## Requirements

### Core Requirements
- ✅ **Command invocation metrics** - Track basic command invocation counters for relevant commands.
- ✅ **OTLP format** - Use OTLP specification as the standard for metric submission to a remote collector. 
- ✅ **Simple counter metric** - Single counter increment per command execution (no duration or outcome tracking)
- ✅ **Best effort philosophy** - Telemetry is best-effort, any failure should not bubble up to the user.
- ✅ **Immediate sending** - Direct HTTP calls with no background processing
- ✅ **Lightweight approach** - Custom OTLP HTTP client (avoid heavy OTLP SDK)
- ✅ **Best-effort with timeout** - Silent failure on timeout
- ✅ **Global telemetry handle** - Single global instance with session state
- ✅ **Anonymous user fingerprint** - Unique but privacy-preserving user ID

### Performance Requirements
- **Zero overhead when disabled** - No telemetry code runs if disabled
- **Minimal overhead when enabled** - Metric creation + immediate HTTP call only
- **No shutdown complexity** - No background tasks to manage
- **Silent failures** - Network/timeout issues don't affect CLI
- **No memory overhead** - No queues or metric collections maintained

## Architecture

### System Components

#### 1. Telemetry Module (`src/telemetry/`)
- **mod.rs**: Entry point with global client initialization and command tracking interface
- **client.rs**: Custom OTLP HTTP client with metric encoding and timeout handling  
- **fingerprint.rs**: Anonymous user fingerprint generation and persistence

#### 2. Global Configuration (`src/global.rs`)
- **TelemetryConfig**: Configuration structure with enabled flag, OTLP endpoint, and timeout
- **Config persistence**: TOML-based config stored in `~/.tx3/trix/config.toml`
- **User notification**: Telemetry info display on first config creation

#### 3. Command Integration
Commands call `track_command_execution("command_name")` at entry point:
- `build`, `check`, `codegen`, `test`, `invoke`, `init`, `devnet`
- Telemetry command for enable/disable/status management

### Data Flow

1. **CLI Startup**: 
   - Load global config from `~/.tx3/trix/config.toml`
   - If enabled and endpoint configured, initialize global `OtlpClient`

2. **Command Execution**:
   - Command calls `track_command_execution(name)`
   - Creates `CommandMetric` with command name
   - Spawns async task to send metric via HTTP POST

3. **Metric Encoding**:
   - Manual OTLP JSON format (no heavy SDK dependency)
   - Resource attributes: service name, version, user fingerprint
   - Single metric: `trix_command_invocation` counter with command name label

4. **Network Sending**:
   - Direct HTTP POST with configurable timeout (default 2s)
   - Silent failure on timeout/network errors
   - No background processing or queuing

### OTLP Metrics Format

The system sends a single counter metric in OTLP JSON format:

```json
{
  "resourceMetrics": [{
    "resource": {
      "attributes": [
        {"key": "service.name", "value": {"stringValue": "trix-cli"}},
        {"key": "service.version", "value": {"stringValue": "0.19.3"}},
        {"key": "user.fingerprint", "value": {"stringValue": "abc123..."}}
      ]
    },
    "scopeMetrics": [{
      "scope": {},
      "metrics": [{
        "name": "trix_command_invocation",
        "sum": {
          "dataPoints": [{
            "attributes": [
              {"key": "command_name", "value": {"stringValue": "build"}}
            ],
            "startTimeUnixNano": "1640995200000000000",
            "timeUnixNano": "1640995200000000000", 
            "asInt": "1"
          }],
          "aggregationTemporality": 2,
          "isMonotonic": true
        }
      }]
    }]
  }]
}
```

**Metric Details:**
- **Name**: `trix_command_invocation`
- **Type**: Counter sum with monotonic flag
- **Labels**: `command_name` (e.g., "build", "test", "invoke")
- **Value**: Always "1" (single increment per execution)
- **Aggregation**: Cumulative (temporarily 2)

### Privacy Design

#### User Fingerprint (`fingerprint.rs`)

- **Generation**: Hash of hostname, OS/arch, and process-specific randomness
- **Persistence**: Stored in `~/.tx3/trix/fingerprint` 
- **Anonymous**: No personal identifiers, stable across sessions
- **Fallback**: Regenerates if storage fails

#### Data Collection

- **Command names only**: No arguments, file paths, or sensitive data
- **Counter increments only**: No success/failure tracking, no duration tracking
- **No PII**: No usernames, file contents, or project information

### Performance Characteristics

- **Zero overhead when disabled**: Early return if config disabled
- **Minimal when enabled**: Metric creation + single HTTP POST
- **Async sending**: Non-blocking to command execution
- **Timeout protection**: Configurable timeout prevents hangs
- **No memory accumulation**: No metric queues or collections

### Error Handling

- **Best effort philosophy**: All telemetry failures are silent
- **Network timeouts**: Handled gracefully with `Result<(), ()>` 
- **Initialization failures**: Warnings logged but don't prevent CLI usage
- **Storage failures**: Fingerprint generation fails gracefully to new fingerprint
