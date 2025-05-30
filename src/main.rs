use serde::{Serialize, Deserialize};
use v8; // rusty_v8 を v8 に変更

// get_error は HandleScope と例外の Local を取るように変更
fn get_error(scope: &mut v8::HandleScope, exception: v8::Local<v8::Value>) -> String {
    let message = v8::Exception::create_message(scope, exception);
    // message.get(scope) は v8::Local<v8::String> を返す
    message.get(scope).to_rust_string_lossy(scope)
}

fn exec_v8(input: &str) -> Result<String, String> {
    // Isolateの作成方法を変更
    let mut params = v8::Isolate::create_params();
    // 必要であれば params をカスタマイズ
    // params.set_array_buffer_allocator(...); // 例
    let mut isolate = v8::Isolate::new(params);

    let handle_scope = &mut v8::HandleScope::new(&mut isolate);
    let context = v8::Context::new(handle_scope);
    let context_scope = &mut v8::ContextScope::new(handle_scope, context);

    // TryCatchスコープを作成
    let tc_scope = &mut v8::TryCatch::new(context_scope);

    let code_local = match v8::String::new(tc_scope, input) {
        Some(s) => s,
        // String::new が None を返すのは通常メモリ不足など深刻な場合。
        None => return Err("Failed to create V8 string from input. Potential OOM.".to_string()),
    };

    // code.to_rust_string_lossy(scope); // この行は不要。

    let script = match v8::Script::compile(tc_scope, code_local, None) {
        Some(s) => s,
        None => { // コンパイルエラー
            if tc_scope.has_caught() {
                let exception = tc_scope.exception().unwrap();
                return Err(get_error(tc_scope, exception));
            } else {
                return Err("Unknown error during script compilation without exception.".to_string());
            }
        }
    };

    match script.run(tc_scope) {
        Some(value) => {
            match value.to_string(tc_scope) {
                Some(s_val) => Ok(s_val.to_rust_string_lossy(tc_scope)),
                None => {
                    if tc_scope.has_caught() {
                        let exception = tc_scope.exception().unwrap();
                        Err(get_error(tc_scope, exception))
                    } else {
                        Err("Execution result cannot be converted to string (and no exception was thrown).".to_string())
                    }
                }
            }
        },
        None => { // ランタイムエラー
            if tc_scope.has_caught() {
                let exception = tc_scope.exception().unwrap();
                Err(get_error(tc_scope, exception))
            } else {
                Err("Unknown error during script execution without exception.".to_string())
            }
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
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();
    
    let mut input_str = String::new();
    if let Err(e) = std::io::stdin().read_line(&mut input_str) { // エラー内容を取得
        let result = serde_json::to_string(&ScriptResult {
            result: "".to_string(),
            error: format!("Error reading input: {}", e) // 具体的なエラーメッセージ
        }).unwrap_or_else(|se| format!("{{\"result\":\"\",\"error\":\"Error reading input and failed to serialize error: {}. Original error: {}\"}}", se, e));
        print!("{}", result);
        return;
    }

    // BOMの除去
    let input_str = if input_str.starts_with('\u{feff}') {
        input_str.trim_start_matches('\u{feff}').to_string()
    } else {
        input_str
    };

    let input: Input = match serde_json::from_str(&input_str) {
        Ok(i) => i,
        Err(e) => {
            let result = serde_json::to_string(&ScriptResult {
                result: "".to_string(),
                error: format!("Failed to parse input JSON: {}", e)
            }).unwrap_or_else(|se| format!("{{\"result\":\"\",\"error\":\"Failed to parse input JSON and failed to serialize error: {}. Original error: {}\"}}", se, e));
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
    let result = serde_json::to_string(&res).unwrap_or_else(|e| format!("{{\"result\":\"{}\",\"error\":\"Failed to serialize final result: {}. Original error for script '{}': {}\"}}", res.result, e, script, res.error));
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
        assert!(result.is_ok(), "Expected Ok, got Err: {:?}", result);
        assert_eq!(result.unwrap(), "2");
    }

    #[test]
    fn test_syntax_error() {
        init_v8();
        let result = exec_v8("2 +");
        assert!(result.is_err(), "Expected Err for syntax error, got Ok: {:?}", result);
        // エラーメッセージの確認 (実際のV8のエラーメッセージに合わせる)
        //例: assert!(result.unwrap_err().contains("SyntaxError: Unexpected end of input"));
    }

    #[test]
    fn test_runtime_error() {
        init_v8();
        let result = exec_v8("nonExistentVariable");
        assert!(result.is_err(), "Expected Err for runtime error, got Ok: {:?}", result);
        // エラーメッセージの確認
        //例: assert!(result.unwrap_err().contains("ReferenceError: nonExistentVariable is not defined"));
    }

    #[test]
    fn test_empty_script() {
        init_v8();
        let result = exec_v8("");
        assert!(result.is_ok(), "Expected Ok for empty script, got Err: {:?}", result);
        // V8では空スクリプトの結果は undefined (文字列)
        assert_eq!(result.unwrap(), "undefined");
    }

    #[test]
    fn test_string_result() {
        init_v8();
        let result = exec_v8("'hello world'");
        assert!(result.is_ok(), "Expected Ok for string result, got Err: {:?}", result);
        assert_eq!(result.unwrap(), "hello world");
    }

    #[test]
    fn test_null_result() {
        init_v8();
        let result = exec_v8("null");
        assert!(result.is_ok(), "Expected Ok for null result, got Err: {:?}", result);
        assert_eq!(result.unwrap(), "null");
    }

    #[test]
    fn test_boolean_true_result() {
        init_v8();
        let result = exec_v8("true");
        assert!(result.is_ok(), "Expected Ok for boolean true result, got Err: {:?}", result);
        assert_eq!(result.unwrap(), "true");
    }

    #[test]
    fn test_boolean_false_result() {
        init_v8();
        let result = exec_v8("false");
        assert!(result.is_ok(), "Expected Ok for boolean false result, got Err: {:?}", result);
        assert_eq!(result.unwrap(), "false");
    }

    #[test]
    fn test_number_result() {
        init_v8();
        let result = exec_v8("123.456");
        assert!(result.is_ok(), "Expected Ok for number result, got Err: {:?}", result);
        assert_eq!(result.unwrap(), "123.456");
    }

    #[test]
    fn test_object_result() {
        init_v8();
        // オブジェクトのデフォルトの toString は "[object Object]"
        let result = exec_v8("({a: 1})");
        assert!(result.is_ok(), "Expected Ok for object result, got Err: {:?}", result);
        assert_eq!(result.unwrap(), "[object Object]");
    }

    #[test]
    fn test_array_result() {
        init_v8();
        // 配列のデフォルトの toString は要素をカンマ区切りにしたもの
        let result = exec_v8("[1, 'two', true]");
        assert!(result.is_ok(), "Expected Ok for array result, got Err: {:?}", result);
        assert_eq!(result.unwrap(), "1,two,true");
    }

    #[test]
    fn test_function_result() {
        init_v8();
        // 関数の toString は関数定義の文字列
        let result = exec_v8("(function() { return 1; })");
        assert!(result.is_ok(), "Expected Ok for function result, got Err: {:?}", result);
        // 関数の文字列表現はV8のバージョンや設定によって微妙に変わることがあるため、柔軟なチェック
        let s = result.unwrap();
        assert!(s.starts_with("function") || s.starts_with("(function"));
        assert!(s.contains("return 1"));
    }

    #[test]
    fn test_error_object_throw() {
        init_v8();
        let result = exec_v8("throw new Error('test error')");
        assert!(result.is_err(), "Expected Err for thrown error object, got Ok: {:?}", result);
        let err_msg = result.unwrap_err();
        // V8の実際のエラーメッセージに合わせて調整
        // "Uncaught Error: test error" のような形式になることが多い
        assert!(err_msg.contains("Error: test error") || err_msg.contains("Uncaught Error: test error"));
    }

    #[test]
    fn test_string_throw() {
        init_v8();
        let result = exec_v8("throw 'a string error'");
        assert!(result.is_err(), "Expected Err for thrown string, got Ok: {:?}", result);
        // V8 は文字列をそのままエラーメッセージとして使うか、"Uncaught a string error" のようにラップすることがある
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("a string error"));
    }

    #[test]
    fn test_long_script_execution() {
        init_v8();
        // 簡単なループだが、あまりにも巨大なものはタイムアウトする可能性があるため注意
        // ここでは適度な長さのスクリプトで正常終了を確認
        let result = exec_v8("let a = 0; for(let i = 0; i < 1000; i++) { a += i; }; a;");
        assert!(result.is_ok(), "Expected Ok for long script, got Err: {:?}", result);
        assert_eq!(result.unwrap(), "499500"); // 0 から 999 までの合計
    }
}