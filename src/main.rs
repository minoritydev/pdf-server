use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;

#[derive(Clone)]
struct AppState {
    client: Arc<Mutex<Client>>,
}

async fn not_found() -> impl Responder {
    HttpResponse::NotFound().body("404 - Route not found")
}


async fn download_pdf(state: web::Data<AppState>, filename: web::Path<String>) -> impl Responder {
    // this is stored in oracle vault, need to figure out a way to retrieve
    // ask me for this string for nows
    let par_string = "" 
    let s3_url = format!("https://objectstorage.eu-amsterdam-1.oraclecloud.com/p/{}/n/axazr4elhg0l/b/pdfstore/o/{}", par_string,filename);
    let client = state.client.lock().await;

    match client.get(&s3_url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                let body = response.bytes().await.unwrap();

                HttpResponse::Ok()
                    .content_type("application/pdf")
                    .body(body)
            } else {
                let body = response.bytes().await.unwrap();
                HttpResponse::InternalServerError().body(body)
            }
        }
        Err(e) => {
            log::error!("Error fetching PDF: {}", e);
            HttpResponse::InternalServerError().body("Error fetching PDF")
        }
    }
}

async fn list_pdfs(state: web::Data<AppState>) -> impl Responder {
    let client = state.client.lock().await;
    let s3_url = "https://objectstorage.eu-amsterdam-1.oraclecloud.com/p/M-p0M8GoAk7_M5Gp4_-j_KBWyPQ6gt1rXPapl0MKWzo5h4Ms9UWBaBRaF3bgPOHf/n/axazr4elhg0l/b/pdfstore/o";
    let response = client.get(s3_url).send().await.unwrap();
    let body = response.text().await.unwrap();
    HttpResponse::Ok().body(body)
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    
    let client = Client::new();
    let shared_state = web::Data::new(AppState {
        client: Arc::new(Mutex::new(client)),
    });

    HttpServer::new(move || {
        App::new()
            .app_data(shared_state.clone())
            .route("/download/{filename}", web::get().to(download_pdf))
            .route("/list", web::get().to(list_pdfs))
            .default_service(web::to(not_found))
    })
    .workers(4)  
    .max_connections(100)  
    .keep_alive(Duration::from_secs(60)) 
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
 
