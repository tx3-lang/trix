# OTLP Telemetry Implementation Specification

## Overview

This specification outlines the implementation of a lightweight OTLP (OpenTelemetry Protocol) telemetry system for the Trix CLI. The system is designed to track the first user-triggered command in each CLI session with immediate HTTP sending and minimal performance impact.

## Requirements

### Core Requirements
- ✅ **Command invokation metrics** - Track basic command invocation metrics for relevant commands.
- ✅ **Command error span** - Track the execution span only of commands resulting on errors
- ✅ **Best effort philosophy** - Telemetry is best-effort, any failure should not buble up to the user.
- ✅ **Immediate sending** - Direct HTTP calls with no background processing
- ✅ **Lightweight approach** - Custom OTLP HTTP client (avoid heavy OTLP SDK)
- ✅ **Best-effort with timeout** - Silent failure on timeout
- ✅ **Global telemetry handle** - Single global instance with session state
- ✅ **Simple error code field** - String field in payload (no custom error types)
- ✅ **Anonymous user fingerprint** - Unique but privacy-preserving user ID

### Performance Requirements
- **Zero overhead when disabled** - No telemetry code runs if disabled
- **Minimal overhead when enabled** - Span creation + immediate HTTP call only
- **No shutdown complexity** - No background tasks to manage
- **Silent failures** - Network/timeout issues don't affect CLI
- **No memory overhead** - No queues or span collections maintained

## Architecture

### System Components

#### 1. Telemetry Module (`src/telemetry/`)
- **mod.rs**: Entry point with global client initialization and command tracking interface
- **client.rs**: Custom OTLP HTTP client with span encoding and timeout handling  
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
   - Creates `CommandSpan` with start time
   - Spawns async task to send span via HTTP POST

3. **Span Encoding**:
   - Manual OTLP JSON format (no heavy SDK dependency)
   - Resource attributes: service name, version, user fingerprint
   - Span attributes: command name, success status, error code, duration

4. **Network Sending**:
   - Direct HTTP POST with configurable timeout (default 2s)
   - Silent failure on timeout/network errors
   - No background processing or queuing

### Privacy Design

#### User Fingerprint (`fingerprint.rs`)

- **Generation**: Hash of hostname, OS/arch, and process-specific randomness
- **Persistence**: Stored in `~/.tx3/trix/fingerprint` 
- **Anonymous**: No personal identifiers, stable across sessions
- **Fallback**: Regenerates if storage fails

#### Data Collection

- **Command names only**: No arguments, file paths, or sensitive data
- **Error codes only**: String error descriptions, no stack traces
- **No PII**: No usernames, file contents, or project information

### Performance Characteristics

- **Zero overhead when disabled**: Early return if config disabled
- **Minimal when enabled**: Span creation + single HTTP POST
- **Async sending**: Non-blocking to command execution
- **Timeout protection**: Configurable timeout prevents hangs
- **No memory accumulation**: No span queues or collections

### Error Handling

- **Best effort philosophy**: All telemetry failures are silent
- **Network timeouts**: Handled gracefully with `Result<(), ()>` 
- **Initialization failures**: Warnings logged but don't prevent CLI usage
- **Storage failures**: Fingerprint generation fails gracefully to new fingerprint
