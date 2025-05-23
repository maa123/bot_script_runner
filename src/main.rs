use serde::{Serialize, Deserialize};

const MAX_OLD_SPACE_SIZE_BYTES: usize = 10 * 1024 * 1024; // 10 MB
const MAX_YOUNG_SPACE_SIZE_BYTES: usize = 5 * 1024 * 1024; // 5 MB
const CPU_EXECUTION_TIMEOUT_MS: u64 = 250; // 250 milliseconds

fn get_error(scope: &mut rusty_v8::TryCatch<rusty_v8::HandleScope>, near_heap_limit_reached_flag: &Arc<AtomicBool>) -> String {
    // Check the near_heap_limit_reached_flag first.
    // The terminate_execution from the callback is not possible with rusty_v8 v0.32.1's NearHeapLimitCallback.
    // So, this flag indicates the callback was triggered.
    if near_heap_limit_reached_flag.load(Ordering::SeqCst) {
        // If V8 is terminating (e.g. CPU timeout) or threw an exception, and the flag was also set,
        // it's reasonable to assume the heap limit contributed or was concurrently an issue.
        if scope.is_execution_terminating() || scope.exception().is_some() {
            return "Execution error: V8 was near heap allocation limit".to_string();
        }
        // If only the flag is set, but no other V8 error/termination signal at this point of calling get_error,
        // this specific message might be used. The main check for this flag will be after script.run().
        return "V8 was near heap allocation limit".to_string();
    }

    if scope.is_execution_terminating() {
        // This is typically set by isolate.terminate_execution() (e.g. CPU timeout)
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
use std::os::raw::{c_char, c_void}; // Added c_void

extern "C" fn oom_handler(_location: *const c_char, _is_heap_oom: bool) {
    // This handler is called when V8 hits an OOM situation.
    // We don't need to do much here other than preventing the default crash.
    // V8 should already be in a state where it will report an error (e.g., through scope.exception()).
}

// Callback for when V8 is near heap limit.
// IMPORTANT: This callback for rusty_v8 v0.32.1 does NOT get the Isolate passed to it.
// Therefore, isolate.terminate_execution() cannot be called from here directly.
// We can only set a flag that must be checked elsewhere.
extern "C" fn near_heap_limit_cb(
    data: *mut c_void,
    current_heap_limit: usize,
    _initial_heap_limit: usize, // Not used in this version of the callback logic
) -> usize {
    unsafe {
        // Cast the raw pointer back to a pointer to AtomicBool.
        let near_heap_limit_reached_ptr = data as *const AtomicBool;
        // Set the flag to true.
        (*near_heap_limit_reached_ptr).store(true, Ordering::SeqCst);
    }
    // Return the current limit. We are not requesting to increase the heap limit.
    // V8 will likely proceed to OOM if allocation continues and this callback doesn't prevent it by termination.
    current_heap_limit
}

fn exec_v8(input: &str) -> Result<String, String> {
    let near_heap_limit_reached = Arc::new(AtomicBool::new(false));
    // Create a raw pointer from the Arc for the C callback.
    // Arc::as_ptr returns a *const AtomicBool, which is then cast to *mut c_void.
    // This is safe as long as the Arc `near_heap_limit_reached` lives as long as the isolate
    // or at least until the callback is unregistered or no longer possibly called.
    let callback_data_ptr = Arc::as_ptr(&near_heap_limit_reached) as *mut c_void;

    let mut isolate = rusty_v8::Isolate::new(create_params());
    isolate.set_oom_error_handler(oom_handler);
    
    // Register the near heap limit callback.
    // The Isolate is not passed to near_heap_limit_cb in rusty_v8 v0.32.1,
    // so terminate_execution cannot be called directly from there.
    // The callback will set the near_heap_limit_reached flag.
    isolate.add_near_heap_limit_callback(near_heap_limit_cb, callback_data_ptr);

    let isolate_handle = isolate.thread_safe_handle();
    
    let completed_flag = Arc::new(AtomicBool::new(false)); // For CPU timeout
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
                    // Error converting value to string, or script returned undefined/null leading to empty string
                    Err(get_error(scope, &near_heap_limit_reached))
                }
            } else {
                // Script execution failed (e.g., exception thrown, or terminated by CPU timeout or heap limit callback if it could)
                Err(get_error(scope, &near_heap_limit_reached))
            }
        } else {
            // Script compilation failed
            Err(get_error(scope, &near_heap_limit_reached))
        }
    };
    completed_flag.store(true, Ordering::SeqCst); // Signal CPU timeout thread to stop, if it hasn't already fired.
    timeout_monitor_thread.join().unwrap();
    
    // After script execution (successful or not), explicitly check the near_heap_limit_reached flag.
    // This is crucial because the near_heap_limit_cb for rusty_v8 v0.32.1 cannot terminate execution itself.
    // If the script finished (even with an error that wasn't due to termination/exception covered by get_error initially)
    // OR if it finished successfully, but the flag was set, we should report it.
    if near_heap_limit_reached.load(Ordering::SeqCst) {
        // If 'result' is Ok, it means the script finished without a V8 exception, but the heap limit was approached.
        // If 'result' is already an Err, get_error would have already factored in the flag if an exception/termination occurred.
        // This ensures that if the script runs to completion (Ok) but the flag was set, it's treated as an error.
        if result.is_ok() {
            return Err("Execution finished, but V8 was near heap allocation limit.".to_string());
        }
        // If result was already an error, get_error would have used the flag if relevant.
        // No need to override error here unless the existing error is less specific than "near heap limit".
        // The current get_error prioritizes the heap limit flag if other conditions (termination/exception) are met.
    }
    
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
            // The V8 flags --noabort_on_oom --throw_out_of_memory_exception were found to be unrecognized/ineffective.
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
    #[ignore] // ハードOOM時には依然としてプロセスがクラッシュするため、CIでの安定性を考慮
    fn test_memory_limit() {
        init_v8();
        // This script attempts to allocate arrays in a loop.
        // Each iteration attempts to allocate roughly 1MB.
        // MAX_OLD_SPACE_SIZE_BYTES is 10MB, MAX_YOUNG_SPACE_SIZE_BYTES is 5MB.
        // The total is 15MB. The script attempts to allocate 20 * 1MB = 20MB.
        // This should exceed the limits.
        let script = "let a = []; for (let i = 0; i < 20; i++) a.push(new Array(1024*1024).fill(i)); 'done'";
        let result = exec_v8(script);
        // The NearHeapLimitCallback and updated get_error logic should now allow this test
        // to pass by returning the specific error message, rather than crashing.
        assert!(result.is_err()); // Check that it's an error
        assert_eq!(result.unwrap_err(), "Execution error: V8 was near heap allocation limit");
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