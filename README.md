# bot_script_runner

[![Docker](https://github.com/maa123/bot_script_runner/actions/workflows/docker-publish.yml/badge.svg)](https://github.com/maa123/bot_script_runner/actions/workflows/docker-publish.yml)

kitakitsune_botの#script実行用API

## Features

### Resource Limits
The Rust JavaScript execution engine now includes built-in safety features:

- **CPU Time Limiting**: Configurable timeout (default: 300ms) to prevent infinite loops
- **Memory Protection**: Configurable heap size limits (default: 16MB) to prevent memory exhaustion  
- **Graceful Error Handling**: Resource limit violations return error messages instead of crashing the process
- **Thread-Safe Termination**: V8 isolate termination using interrupt mechanisms

### Configuration
Resource limits can be configured through the `ResourceLimits` struct:

```rust
ResourceLimits {
    max_execution_time_ms: 300,  // 300ms timeout
    max_heap_size_bytes: 16 * 1024 * 1024,  // 16MB memory limit
}
```

### Safety Guarantees
- JavaScript execution is safely sandboxed within the Rust process
- No external process control required for resource management
- Process remains stable even under resource exhaustion
- Compatible with existing Go server integration
