use serde::{Serialize, Deserialize};

const MAX_OLD_SPACE_SIZE_BYTES: usize = 10 * 1024 * 1024; // 10 MB
const MAX_YOUNG_SPACE_SIZE_BYTES: usize = 5 * 1024 * 1024; // 5 MB
const CPU_EXECUTION_TIMEOUT_MS: u64 = 250; // 250 milliseconds

fn get_error(scope: &mut rusty_v8::TryCatch<rusty_v8::HandleScope>) -> String {
    if scope.is_execution_terminating() {
        "Execution timed out".to_string()
    } else if let Some(exp) = scope.exception() {
        rusty_v8::Exception::create_message(scope, exp).get(scope).to_rust_string_lossy(scope)
    } else {
        "Unknown error".to_string()
    }
}

fn create_params() -> rusty_v8::CreateParams {
    let mut params = rusty_v8::CreateParams::default();
    // In rusty_v8 v0.32.1, heap_limits takes initial and max for the total heap.
    // We'll sum our defined old and young space for the max. Initial can be 0.
    params = params.heap_limits(0, MAX_OLD_SPACE_SIZE_BYTES + MAX_YOUNG_SPACE_SIZE_BYTES);
    params
}

use std::thread;
use std::time::Duration;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::os::raw::c_char;

extern "C" fn oom_handler(_location: *const c_char, _is_heap_oom: bool) {
    // This handler is called when V8 hits an OOM situation.
    // We don't need to do much here other than preventing the default crash.
    // V8 should already be in a state where it will report an error (e.g., through scope.exception()).
    // For debugging, one could print a message:
    // eprintln!("OOM Handler: location {:?}, is_heap_oom {}", location, is_heap_oom);
}

fn exec_v8(input: &str) -> Result<String, String> {
    let mut isolate = rusty_v8::Isolate::new(create_params());
    isolate.set_oom_error_handler(oom_handler);
    let isolate_handle = isolate.thread_safe_handle();
    
    let completed_flag = Arc::new(AtomicBool::new(false));
    let completed_flag_clone = Arc::clone(&completed_flag);

    let timeout_monitor_thread = thread::spawn(move || {
        thread::sleep(Duration::from_millis(CPU_EXECUTION_TIMEOUT_MS));
        if !completed_flag_clone.load(Ordering::SeqCst) {
            isolate_handle.terminate_execution();
        }
    });

    let result = {
        let base_scope = &mut rusty_v8::HandleScope::new(&mut isolate);
        let context = rusty_v8::Context::new(base_scope);
        let context_scope = &mut rusty_v8::ContextScope::new(base_scope, context);
        let scope = &mut rusty_v8::TryCatch::new(context_scope);
        let code_str = rusty_v8::String::new(scope, &input).unwrap();
        // The line `code.to_rust_string_lossy(scope)` was removed as it's a no-op.
        if let Some(script) = rusty_v8::Script::compile(scope, code_str, None) {
            if let Some(val) = script.run(scope) {
                if let Some(result_str_v8) = val.to_string(scope) {
                    Ok(result_str_v8.to_rust_string_lossy(scope))
                } else {
                    Err(get_error(scope))
                }
            } else {
                Err(get_error(scope))
            }
        } else {
            Err(get_error(scope))
        }
    };
    completed_flag.store(true, Ordering::SeqCst);
    timeout_monitor_thread.join().unwrap();
    result
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
    let platform = rusty_v8::new_default_platform(0, false).make_shared();
    rusty_v8::V8::initialize_platform(platform);
    rusty_v8::V8::initialize();
    
    let mut input_str = String::new();
    if let Err(_) = std::io::stdin().read_line(&mut input_str) {
        let result = serde_json::to_string(&ScriptResult {
            result: "".to_string(),
            error: "Error".to_string()
        }).unwrap();
        print!("{}", result)
    }
    let input: Input = serde_json::from_str(&input_str).unwrap();
    let script = input.script;
    let res = match exec_v8(&script) {
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
            let platform = rusty_v8::new_default_platform(0, false).make_shared();
            rusty_v8::V8::initialize_platform(platform);
            rusty_v8::V8::initialize();
        });
    }

    #[test]
    fn test_exec() {
        init_v8();
        let result = exec_v8("1 + 1");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "2");
    }

    #[test]
    fn test_syntax_error() {
        init_v8();
        let result = exec_v8("2 +");
        assert!(result.is_err());
    }

    #[test]
    fn test_runtime_error() {
        init_v8();
        let result = exec_v8("undefined_variable");
        assert!(result.is_err());
    }

    #[test]
    fn test_memory_limit() {
        init_v8();
        // This script attempts to allocate arrays in a loop.
        // Each iteration attempts to allocate roughly 1MB.
        // MAX_OLD_SPACE_SIZE_BYTES is 10MB, MAX_YOUNG_SPACE_SIZE_BYTES is 5MB.
        // The total is 15MB. The script attempts to allocate 20 * 1MB = 20MB.
        // This should exceed the limits.
        let script = "let a = []; for (let i = 0; i < 20; i++) a.push(new Array(1024*1024).fill(i)); 'done'";
        let result = exec_v8(script);
        // Note: V8's default OOM behavior, even with an OOM handler set via
        // rusty_v8 v0.32.1, tends to be a fatal process termination.
        // So, if this test runs in a suite, it might crash the entire suite.
        // However, the assertion is what we'd expect if it *could* return an error.
        assert!(result.is_err(), "Expected memory limit to be exceeded, but script succeeded with: {:?}", result.ok());
    }

    #[test]
    fn test_cpu_timeout() {
        init_v8();
        let script = "while(true) {}";
        let result = exec_v8(script);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Execution timed out");
    }
}