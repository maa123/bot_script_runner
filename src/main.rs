use serde::{Serialize, Deserialize};

fn get_error(scope: &mut v8::TryCatch<v8::HandleScope>) -> String {
    if let Some(exp) = scope.exception() {
        v8::Exception::create_message(scope, exp).get(scope).to_rust_string_lossy(scope)
    } else {
        "".to_string()
    }
}

fn exec_v8(input: &str) -> Result<String, String> {
    let mut isolate = v8::Isolate::new(Default::default());
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
}
