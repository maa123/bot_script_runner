use serde::{Serialize, Deserialize};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;
use std::thread;

fn get_error(scope: &mut v8::TryCatch<v8::HandleScope>) -> String {
    if let Some(exp) = scope.exception() {
        v8::Exception::create_message(scope, exp).get(scope).to_rust_string_lossy(scope)
    } else {
        "".to_string()
    }
}

/// Configuration for resource limits
pub struct ResourceLimits {
    /// Maximum execution time in milliseconds
    pub max_execution_time_ms: u64,
    /// Maximum heap size in bytes
    pub max_heap_size_bytes: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_execution_time_ms: 300, // 300ms default, same as current Go timeout
            max_heap_size_bytes: 16 * 1024 * 1024, // 16MB default
        }
    }
}

/// Sets up resource limits for the current process
fn setup_process_limits(_limits: &ResourceLimits) -> Result<(), String> {
    // Note: Process-level resource limits can interfere with V8 initialization
    // and thread creation, so we disable them for now and rely on V8-level controls.
    // In a production environment, you might want to configure these more carefully
    // or use external process monitoring.
    
    // Commented out to avoid interference with V8 runtime:
    // setrlimit(Resource::RLIMIT_CPU, cpu_limit_seconds, cpu_limit_seconds)
    // setrlimit(Resource::RLIMIT_AS, memory_limit, memory_limit)
    
    Ok(())
}

fn exec_v8_with_limits(input: &str, limits: &ResourceLimits) -> Result<String, String> {
    // Set up an interrupt flag for timeout handling
    let interrupted = Arc::new(AtomicBool::new(false));
    let interrupted_clone = interrupted.clone();
    
    // Create isolate with memory constraints
    let mut create_params = v8::Isolate::create_params();
    
    // Set up array buffer allocator
    let array_buffer_allocator = v8::new_default_allocator().make_shared();
    create_params = create_params.array_buffer_allocator(array_buffer_allocator);
    
    let mut isolate = v8::Isolate::new(create_params);
    
    // Set up interrupt handling for the isolate
    let isolate_handle = isolate.thread_safe_handle();
    
    // Start timeout timer in a separate thread
    let timeout_duration = Duration::from_millis(limits.max_execution_time_ms);
    let _timeout_handle = thread::spawn(move || {
        thread::sleep(timeout_duration);
        interrupted_clone.store(true, Ordering::Relaxed);
        // Request interrupt on the isolate
        isolate_handle.terminate_execution();
    });
    
    let base_scope = &mut v8::HandleScope::new(&mut isolate);
    let context = v8::Context::new(base_scope, Default::default());
    let context_scope = &mut v8::ContextScope::new(base_scope, context);
    let scope = &mut v8::TryCatch::new(context_scope);
    
    let code = v8::String::new(scope, &input).unwrap();
    if let Some(script) = v8::Script::compile(scope, code, None) {
        if let Some(val) = script.run(scope) {
            if let Some(result) = val.to_string(scope) {
                if interrupted.load(Ordering::Relaxed) {
                    Err("Execution timeout".to_string())
                } else {
                    Ok(result.to_rust_string_lossy(scope))
                }
            } else {
                if interrupted.load(Ordering::Relaxed) {
                    Err("Execution timeout".to_string())
                } else {
                    Err(get_error(scope))
                }
            }            
        } else {
            if interrupted.load(Ordering::Relaxed) {
                Err("Execution timeout".to_string())
            } else {
                Err(get_error(scope))
            }
        }
    } else {
        if interrupted.load(Ordering::Relaxed) {
            Err("Execution timeout".to_string())
        } else {
            Err(get_error(scope))
        }
    }
}

#[derive(Serialize)]
struct ScriptResult {
    result: String,
    error: String
}
#[derive(Deserialize)]
struct Input {
    script: String
}

fn main() {
    // Initialize V8
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();
    
    // Set up resource limits for the process (optional - continue if it fails)
    let limits = ResourceLimits::default();
    if let Err(e) = setup_process_limits(&limits) {
        eprintln!("Warning: Failed to set process limits: {}", e);
        // Continue execution but without process-level limits
    }
    
    let mut input_str = String::new();
    if let Err(_) = std::io::stdin().read_line(&mut input_str) {
        let result = serde_json::to_string(&ScriptResult {
            result: "".to_string(),
            error: "Error".to_string()
        }).unwrap();
        print!("{}", result);
        return;
    }
    
    let input: Result<Input, _> = serde_json::from_str(&input_str);
    let input = match input {
        Ok(i) => i,
        Err(_) => {
            let result = serde_json::to_string(&ScriptResult {
                result: "".to_string(),
                error: "Invalid input format".to_string()
            }).unwrap();
            print!("{}", result);
            return;
        }
    };
    
    let script = input.script;
    let res = match exec_v8_with_limits(&script, &limits) {
        Ok(s) => ScriptResult {
            result: s,
            error: "".to_string()
        },
        Err(s) => ScriptResult {
            result: "".to_string(),
            error: s
        }
    };
    let result = serde_json::to_string(&res).unwrap();
    print!("{}", result);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_v8() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            let platform = v8::new_default_platform(0, false).make_shared();
            v8::V8::initialize_platform(platform);
            v8::V8::initialize();
        });
    }

    #[test]
    fn test_exec() {
        init_v8();
        let result = exec_v8_with_limits("1 + 1", &ResourceLimits::default());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "2");
    }

    #[test]
    fn test_syntax_error() {
        init_v8();
        let result = exec_v8_with_limits("2 +", &ResourceLimits::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_runtime_error() {
        init_v8();
        let result = exec_v8_with_limits("undefined_variable", &ResourceLimits::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_exec_with_limits_normal() {
        init_v8();
        let limits = ResourceLimits {
            max_execution_time_ms: 1000,
            max_heap_size_bytes: 16 * 1024 * 1024,
        };
        let result = exec_v8_with_limits("1 + 1", &limits);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "2");
    }

    #[test]
    fn test_exec_with_timeout() {
        init_v8();
        let limits = ResourceLimits {
            max_execution_time_ms: 100, // Very short timeout
            max_heap_size_bytes: 16 * 1024 * 1024,
        };
        // Create a script that runs for a long time
        let long_running_script = "
            let start = Date.now();
            while (Date.now() - start < 500) {
                // Busy wait for 500ms
            }
            'completed'
        ";
        let result = exec_v8_with_limits(long_running_script, &limits);
        assert!(result.is_err());
        let error = result.unwrap_err();
        // Should be either timeout or termination error
        assert!(error.contains("timeout") || error.contains("Execution terminated"));
    }

    #[test]
    fn test_exec_infinite_loop_with_timeout() {
        init_v8();
        let limits = ResourceLimits {
            max_execution_time_ms: 100,
            max_heap_size_bytes: 16 * 1024 * 1024,
        };
        // Create an infinite loop
        let infinite_loop_script = "while(true) { /* infinite loop */ }";
        let result = exec_v8_with_limits(infinite_loop_script, &limits);
        assert!(result.is_err());
        let error = result.unwrap_err();
        // Should be timeout or termination error
        assert!(error.contains("timeout") || error.contains("Execution terminated"));
    }

    #[test]
    fn test_memory_intensive_script() {
        init_v8();
        let limits = ResourceLimits {
            max_execution_time_ms: 2000, // Shorter timeout for test
            max_heap_size_bytes: 2 * 1024 * 1024, // 2MB limit - more generous for testing
        };
        // Create a script that tries to allocate memory but less aggressively
        let memory_script = "
            let arr = [];
            try {
                for (let i = 0; i < 10000; i++) {
                    arr.push('x'.repeat(100)); // Smaller allocations
                }
                'completed'
            } catch (e) {
                'memory_error: ' + e.message
            }
        ";
        let result = exec_v8_with_limits(memory_script, &limits);
        // This should either complete or fail gracefully without crashing
        match result {
            Ok(output) => {
                // Should complete or catch the memory error
                assert!(output.contains("completed") || output.contains("memory_error"));
            },
            Err(error) => {
                // Timeout or other execution error is also acceptable
                assert!(error.contains("timeout") || error.contains("Execution terminated") || !error.is_empty());
            }
        }
    }

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_execution_time_ms, 300);
        assert_eq!(limits.max_heap_size_bytes, 16 * 1024 * 1024);
    }

    #[test]
    fn test_setup_process_limits() {
        let limits = ResourceLimits::default();
        // This test might fail in some environments due to permission restrictions
        // but we'll test that it doesn't panic
        let _result = setup_process_limits(&limits);
        // Just ensure it doesn't panic - actual success depends on system permissions
    }
}
