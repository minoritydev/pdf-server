use actix_web::{web, App, HttpResponse, HttpServer, Responder, middleware::Logger};
use actix_web_httpauth::{middleware::HttpAuthentication, extractors::bearer::BearerAuth};
use reqwest::{Client, header::{HeaderName as ReqwestHeaderName, HeaderMap as reqwestHeaderMap, HeaderValue}};
use std::time::Duration;
use clap::Parser;
use reqsign::oracle;
use bytes::Bytes;
use log::{info, error};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(short, long, required = true)]
    port: u16,
    #[arg(short, long, env = "PS_API_TOKEN")]
    token: String,
}

#[derive(Clone)]
struct AppState {
    client: Client,
    token: String,
}

/* 
Signs the requests we make to Oracle cloud, returns bytes which can be constructed into an HttpRequest by the calling function.
It uses env vars to sign the request, needs the following env vars defined:
OCI_TENANCY
OCI_USER
OCI_FINGERPRINT
OCI_KEY_FILE
*/
async fn send_signed_request(client: &Client, url: &str) -> Result<Bytes,  Box<dyn std::error::Error>>{
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
        let name: ReqwestHeaderName = ReqwestHeaderName::from_bytes(name.as_str().as_bytes())?;
        let value: HeaderValue = HeaderValue::from_bytes(value.as_bytes())?;
        reqwest_headers.insert(name,value,);
    }
    info!("Sending signed request to {}", url);
    let response = client.get(url).headers(reqwest_headers).send().await?.error_for_status()?;
    let bytes = response.bytes().await?;  
    info!("Received response from {}", url);
    Ok(bytes)
}

// validates requests to our own endpoints using OAuth2 headers.
// needs PS_API_TOKEN env var set.
async fn validator(
    req: actix_web::dev::ServiceRequest,
    credentials: BearerAuth,
) -> Result<actix_web::dev::ServiceRequest, (actix_web::Error, actix_web::dev::ServiceRequest)> {
    let state = req.app_data::<web::Data<AppState>>().cloned();
    if let Some(state) = state {
        let expected_token = &state.token;
        let provided_token = credentials.token();

        if provided_token == expected_token {
            info!("Authorized request");
            Ok(req)
        } else {
            error!("Invalid auth token provided");
            HttpResponse::Unauthorized().body("Invalid or missing Bearer token");
            Err((actix_web::error::ErrorUnauthorized("Invalid auth token."), req))
        }
    } else {
        error!("App state missing during validation");
        HttpResponse::InternalServerError().body("Server misconfiguration");
        Err((actix_web::error::ErrorInternalServerError("Server misconfiguration."), req))
    }
}

// Handles invalid routes with 404
async fn not_found() -> impl Responder {
     info!("404 - route not found");
    HttpResponse::NotFound().body("404 - Route not found")
}

// Sends request to oracle S3 to fetch PDF, uses send_signed_request to construct signed HttpRequest.
async fn download_pdf(state: web::Data<AppState>, filename: web::Path<String>) -> impl Responder {
    let client = &state.client;
    let s3_url = format!("https://objectstorage.eu-amsterdam-1.oraclecloud.com/n/axazr4elhg0l/b/pdfstore/o/{}",filename);
    info!("Request received: /download/{}", filename);
    match send_signed_request(&client, &s3_url).await {
        Ok(bytes) =>{ 
            info!("Successfully downloaded PDF: {}", filename);
            HttpResponse::Ok()
            .content_type("application/pdf")
            .body(bytes)
        }
        Err(e) => {
            error!("Error downloading PDF {}: {}", filename, e);
            HttpResponse::InternalServerError().body("Failed to download PDF")
        }
    }
}

// Sends request to oracle S3 to list PDFs, uses send_signed_request to construct signed HttpRequest. 
async fn list_pdfs(state: web::Data<AppState>) -> impl Responder {
    let client = &state.client;
    let s3_url = "https://objectstorage.eu-amsterdam-1.oraclecloud.com/n/axazr4elhg0l/b/pdfstore/o";
      info!("Request received: /list");
    match send_signed_request(&client, &s3_url).await {
        Ok(bytes) => { 
            info!("Successfully listed PDFs");
            HttpResponse::Ok()
            .content_type("application/json")
            .body(bytes)
        }
        Err(e) => {
             error!("Error listing PDFs: {}", e);
            HttpResponse::InternalServerError().body("Failed to list PDFs")
        }
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    let args = Args::parse();
    let client = Client::new();
    let addr = format!("0.0.0.0:{}", args.port);
   info!("pdf-server running on {}", addr);
    let shared_state = web::Data::new(AppState {
        client,
        token: args.token.clone(),
    });
    let auth_middleware = HttpAuthentication::bearer(validator);
    HttpServer::new(move || {
        App::new()
            .wrap(
                Logger::new("%a \"%r\" %s %b bytes %Dms")
            // %a = remote IP
            // %r = first line of the request (method + path + protocol)
            // %s = response status
            // %b = response size in bytes
            // %D = time to serve request in ms
            )
            .app_data(shared_state.clone())
            .service(
                web::scope("")
                .wrap(auth_middleware.clone())
                .route("/download/{filename}", web::get().to(download_pdf))
                .route("/list", web::get().to(list_pdfs))
            )
            .default_service(web::to(not_found))
    })
    .workers(4)  
    .max_connections(100)  
    .keep_alive(Duration::from_secs(60)) 
    .bind(&addr)?
    .run()
    .await
}
 
