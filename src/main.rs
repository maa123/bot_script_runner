use serde::{Serialize, Deserialize};

fn get_error(scope: &mut v8::TryCatch<v8::HandleScope>) -> String {
    if let Some(exp) = scope.exception() {
        v8::Exception::create_message(scope, exp).get(scope).to_rust_string_lossy(scope)
    } else {
        "".to_string()
    }
}

fn exec_v8(input: &str, cpu_limit_ms: u64, heap_limit: usize) -> Result<String, String> {
    let params = v8::Isolate::create_params().heap_limits(0, heap_limit);
    let mut isolate = v8::Isolate::new(params);
    let handle = isolate.thread_safe_handle();

    use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

    // Setup memory limit callback data.
    #[repr(C)]
    struct HeapLimitData {
        handle: v8::IsolateHandle,
        triggered: Arc<AtomicBool>,
    }

    extern "C" fn heap_limit_callback(
        data: *mut std::ffi::c_void,
        current_heap_limit: usize,
        _initial_heap_limit: usize,
    ) -> usize {
        // SAFETY: data is a pointer to HeapLimitData allocated below.
        let data = unsafe { &*(data as *const HeapLimitData) };
        data.triggered.store(true, Ordering::SeqCst);
        data.handle.terminate_execution();
        // Bump heap limit to avoid immediate crash until termination propagates.
        current_heap_limit.saturating_mul(2)
    }

    let mem_triggered = Arc::new(AtomicBool::new(false));
    let heap_data = Box::new(HeapLimitData { handle: handle.clone(), triggered: mem_triggered.clone() });
    let heap_data_ptr = Box::into_raw(heap_data) as *mut std::ffi::c_void;
    isolate.add_near_heap_limit_callback(heap_limit_callback, heap_data_ptr);

    // Setup CPU time watcher thread.
    let finished = Arc::new(AtomicBool::new(false));
    let fin = finished.clone();
    let cpu_handle = handle.clone();
    let watcher = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(cpu_limit_ms));
        if !fin.load(Ordering::SeqCst) {
            cpu_handle.terminate_execution();
        }
    });

    let result = {
        let base_scope = &mut v8::HandleScope::new(&mut isolate);
        let context = v8::Context::new(base_scope, Default::default());
        let context_scope = &mut v8::ContextScope::new(base_scope, context);
        let scope = &mut v8::TryCatch::new(context_scope);
        let code = v8::String::new(scope, &input).unwrap();
        if let Some(script) = v8::Script::compile(scope, code, None) {
            if let Some(val) = script.run(scope) {
                if let Some(result) = val.to_string(scope) {
                    Ok(result.to_rust_string_lossy(scope))
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
    finished.store(true, Ordering::SeqCst);
    let _ = watcher.join();

    isolate.remove_near_heap_limit_callback(heap_limit_callback, heap_limit);
    // SAFETY: heap_data_ptr was allocated above and is no longer used by V8.
    unsafe { drop(Box::from_raw(heap_data_ptr as *mut HeapLimitData)) };

    if mem_triggered.load(Ordering::SeqCst) {
        isolate.cancel_terminate_execution();
        Err("Memory limit".to_string())
    } else if isolate.is_execution_terminating() {
        isolate.cancel_terminate_execution();
        Err("Timeout".to_string())
    } else {
        result
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
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();
    
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
    const CPU_LIMIT_MS: u64 = 300;
    const HEAP_LIMIT: usize = 16 * 1024 * 1024;
    let res = match exec_v8(&script, CPU_LIMIT_MS, HEAP_LIMIT) {
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
        let result = exec_v8("1 + 1", 100, 1024 * 1024);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "2");
    }

    #[test]
    fn test_syntax_error() {
        init_v8();
        let result = exec_v8("2 +", 100, 1024 * 1024);
        assert!(result.is_err());
    }

    #[test]
    fn test_runtime_error() {
        init_v8();
        let result = exec_v8("undefined_variable", 100, 1024 * 1024);
        assert!(result.is_err());
    }

    #[test]
    fn test_timeout() {
        init_v8();
        let result = exec_v8("while(true) {}", 10, 1024 * 1024);
        assert!(result.is_err());
    }

    #[test]
    fn test_memory_limit() {
        init_v8();
        let script = r#"
            let arrays = [];
            for (let i = 0; i < 1_000_000; i++) {
                arrays.push(new Array(1000).fill(Math.random()));
            }
        "#;
        let result = exec_v8(script, 1000, 1024 * 1024); // 1 MB limit
        assert!(result.is_err());
    }
}
