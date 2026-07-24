use axum::{
    body::Body,
    http::{StatusCode, Uri, header},
    response::Response,
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../webfile"]
pub struct StaticAssets;

pub async fn static_handler(uri: Uri) -> Response {
    let mut path = uri.path().trim_start_matches('/').to_string();

    if path.is_empty() {
        path = "index.html".to_string();
    }

    let resolved = if let Some(content) = StaticAssets::get(&path) {
        Some((content, path.clone()))
    } else if let Some(content) = StaticAssets::get(&format!("{}.html", path)) {
        Some((content, format!("{}.html", path)))
    } else {
        StaticAssets::get(&format!("{}/index.html", path))
            .map(|content| (content, format!("{}/index.html", path)))
    };

    match resolved {
        Some((content, resolved_path)) => {
            let mime = mime_guess::from_path(&resolved_path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Internal Server Error"))
                        .unwrap()
                })
        }
        None => {
            // Fallback to 404.html
            if let Some(content) = StaticAssets::get("404.html") {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .header(header::CONTENT_TYPE, "text/html")
                    .body(Body::from(content.data))
                    .unwrap_or_else(|_| {
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Body::from("Internal Server Error"))
                            .unwrap()
                    })
            } else {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("404 Not Found"))
                    .unwrap()
            }
        }
    }
}
