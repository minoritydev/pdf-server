use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use reqwest::{Client, header::{HeaderName as ReqwestHeaderName, HeaderMap as reqwestHeaderMap, HeaderValue}};
use http::{Request, request::Parts, Method, HeaderMap as httpHeaderMap};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;
use clap::Parser;
use reqsign::oracle;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(short, long, required = true)]
    port: u16,
}

#[derive(Clone)]
struct AppState {
    client: Arc<Mutex<Client>>,
}

async fn signer(url: &str) -> Result<reqwestHeaderMap,  Box<dyn std::error::Error>>{
    let signer = oracle::default_signer();
    let mut req = http::Request::builder()
        .method(http::Method::GET)
        .uri(url)
        .body(())
        .unwrap()
        .into_parts()
        .0;
    signer.sign(&mut req, None).await?;
    let mut reqwest_headers = reqwestHeaderMap::new();
    for (name, value) in req.headers.iter() {
        let name: ReqwestHeaderName = ReqwestHeaderName::from_bytes(name.as_str().as_bytes())
            .expect("invalid header name");
        let value: HeaderValue = HeaderValue::from_bytes(value.as_bytes()).expect("invalid header value");
        reqwest_headers.insert(
            name,
            value,
        );
    }
    Ok(reqwest_headers)
}


async fn not_found() -> impl Responder {
    HttpResponse::NotFound().body("404 - Route not found")
}


async fn download_pdf(state: web::Data<AppState>, filename: web::Path<String>) -> impl Responder {
    let client = state.client.lock().await;
    let par_string_secret_url = "https://secrets.eu-amsterdam-1.oci.oraclecloud.com/20190301/secrets/ocid1.vaultsecret.oc1.eu-amsterdam-1.amaaaaaajdltdoaame2fqdui3j545ze22ka5z6zknzm7hi5x3odsw7p2dvla/content";
    let signed_headers = match signer(&par_string_secret_url).await {
    Ok(parts) => parts,
    Err(e) => {
        eprintln!("Failed to sign request: {:?}", e);
        return HttpResponse::InternalServerError().body("Signing OCI request failed.");
    }
    };
    println!("{:#?}", signed_headers);
    let response = match client.get(par_string_secret_url).headers(signed_headers).send().await {
        Ok(resp) => resp,
        Err(e) => {
            log::error!("Failed to fetch signed URL: {}", e);
            return HttpResponse::InternalServerError().body("Failed to fetch signed URL");
        }
    };
    
    let par_string = match response.text().await {
        Ok(text) => text,
        Err(e) => {
            log::error!("Failed to read response text: {}", e);
            return HttpResponse::InternalServerError().body("Failed to read signed URL response");
        }
    };
    println!("{:#?}", par_string);
    let s3_url = format!("https://objectstorage.eu-amsterdam-1.oraclecloud.com/p/{}/n/axazr4elhg0l/b/pdfstore/o/{}", par_string,filename);

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
    
    let args = Args::parse();
    let client = Client::new();
    let addr = format!("0.0.0.0:{}", args.port);
    println!("pdf-server running on {}", addr );
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
    .bind(&addr)?
    .run()
    .await
}
 
