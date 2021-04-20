use serde::{Deserialize, Serialize};
use actix_web::{ post, web, App, HttpResponse, HttpServer, Responder, Result};
use boa::{exec::Executable, parse, Context};

fn exec(src: &str) -> Result<String, String> {
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
    let res = match exec(&params.script) {
        Ok(s) => ScriptResult {
            result: s,
            error: "".to_string()
        },
        Err(e) => ScriptResult {
            result: "".to_string(),
            error: e
        }
    };
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
    .bind("127.0.0.1:8080")?
    .run()
    .await
}