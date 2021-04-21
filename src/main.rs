use serde::{Serialize, Deserialize};
use boa::{exec::Executable, parse, Context};

fn exec(src: String) -> Result<String, String> {
    let mut context = Context::new();
    let expr = match parse(src, false) {
        Ok(res) => res,
        Err(e) => {
            return Err(format!(
                "Parse Uncaught {}",
                context
                    .throw_syntax_error(e.to_string())
                    .expect_err("interpreter.throw_syntax_error() did not return an error")
                    .display()
            )
            .into());
        }
    };
    expr.run(&mut context)
        .map_err(|e| format!("Uncaught {}", e.display()))
        .map(|v| v.display().to_string())
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
    let mut input_str = String::new();
    if let Err(_) = std::io::stdin().read_line(&mut input_str) {
        let result = serde_json::to_string(&ScriptResult {
            result: "".to_string(),
            error: "Error".to_string()
        }).unwrap();
        print!("{}", result)
    }
    let input: Input = serde_json::from_str(&input_str).unwrap();
    let str = input.script;
    let res = match exec(str.to_string()){
        Ok(s) => ScriptResult {
            result: s,
            error: "".to_string()
        },
        Err(e) => ScriptResult {
            result: "".to_string(),
            error: e
        }
    };
    let result = serde_json::to_string(&res).unwrap();
    print!("{}", result);
}