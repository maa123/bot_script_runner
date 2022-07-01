use serde::{Serialize, Deserialize};

fn get_error(scope: &mut rusty_v8::TryCatch<rusty_v8::HandleScope>) -> String {
    if let Some(exp) = scope.exception() {
        rusty_v8::Exception::create_message(scope, exp).get(scope).to_rust_string_lossy(scope)
    } else {
        "".to_string()
    }
}

fn exec_v8(input: &str) -> Result<String, String> {
    let mut isolate = rusty_v8::Isolate::new(Default::default());
    let base_scope = &mut rusty_v8::HandleScope::new(&mut isolate);
    let context = rusty_v8::Context::new(base_scope);
    let context_scope = &mut rusty_v8::ContextScope::new(base_scope, context);
    let scope = &mut rusty_v8::TryCatch::new(context_scope);
    let code = rusty_v8::String::new(scope, &input).unwrap();
    code.to_rust_string_lossy(scope);
    if let Some(script) = rusty_v8::Script::compile(scope, code, None) {
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
    let platform = rusty_v8::new_default_platform().make_shared();
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