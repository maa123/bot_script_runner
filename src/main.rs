use serde::{Serialize, Deserialize};

fn exec_v8(input: &str) -> Result<String, String> {
    let mut isolate = v8::Isolate::new(Default::default());
    v8::scope!(let scope, &mut isolate);
    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);
    v8::tc_scope!(let scope, scope);
    let code = v8::String::new(scope, input)
        .ok_or_else(|| "Failed to create V8 string from input".to_string())?;
    if let Some(script) = v8::Script::compile(scope, code, None) {
        if let Some(val) = script.run(scope) {
            Ok(val.to_rust_string_lossy(scope))
        } else {
            Err(scope.exception().map(|exp| exp.to_rust_string_lossy(scope)).unwrap_or_default())
        }
    } else {
        Err(scope.exception().map(|exp| exp.to_rust_string_lossy(scope)).unwrap_or_default())
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
    if std::io::stdin().read_line(&mut input_str).is_err() {
        let result = serde_json::to_string(&ScriptResult {
            result: "".to_string(),
            error: "Failed to read input".to_string()
        }).expect("Failed to serialize error result");
        print!("{}", result);
        return;
    }
    let input: Input = match serde_json::from_str(&input_str) {
        Ok(v) => v,
        Err(e) => {
            let result = serde_json::to_string(&ScriptResult {
                result: "".to_string(),
                error: format!("Failed to parse input: {}", e)
            }).expect("Failed to serialize error result");
            print!("{}", result);
            return;
        }
    };
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
    let result = serde_json::to_string(&res).expect("Failed to serialize result");
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
        let result = exec_v8("1 + 1");
        assert!(result.is_ok());
        assert_eq!(result.expect("exec_v8 should succeed"), "2");
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
}
