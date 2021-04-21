use serde::{Deserialize, Serialize};
use actix_web::{ post, web, App, HttpResponse, HttpServer, Responder, Result};
use boa::{exec::Executable, parse, Context};
use tokio::task;

async fn exec(src: String) -> Result<String, String> {
    let res = task::spawn_blocking(move || {
        exec_a(src)
    }).await;
    let resp = match res {
        Ok(r) => r,
        Err(_e) => Err("".to_string()),
    };
    resp
}

fn exec_a(src: String) -> Result<String, String> {
    // Setup executor
    let mut context = Context::new();
    let expr = match parse(src, false) {
        Ok(res) => res,
        Err(e) => {
            return Err(format!(
                "Uncaught {}",
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

#[derive(Serialize, Deserialize)]
pub struct FormParams {
    script: String,
}

#[derive(Serialize)]
struct ScriptResult {
    result: String,
    error: String
}


#[post("/")]
async fn script(params: web::Form<FormParams>) -> Result<HttpResponse> {
    let res: ScriptResult;
    let script_str = (&params.script).to_string();
    
    if let Ok(ar) = tokio::time::timeout(std::time::Duration::from_millis(50), exec(script_str)).await {
        res = match ar {
            Ok(s) => ScriptResult {
                result: s,
                error: "".to_string()
            },
            Err(e) => ScriptResult {
                result: "".to_string(),
                error: e
            }
        }
    } else {
        res = ScriptResult {
            result: "".to_string(),
            error: "Timeout".to_string()
        }
    }
    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .json(res))
}


async fn default() -> impl Responder {
    HttpResponse::Ok().body("200 OK")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(script)
            .route("/", web::get().to(default))
    })
    .client_timeout(500)
    .workers(1)
    .bind("0.0.0.0:7690")?
    .run()
    .await
}