use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../webfile/"]
struct Asset;

pub async fn index() -> Response {
    serve_embedded_path("/").into_response()
}

pub async fn serve(uri: Uri) -> Response {
    serve_embedded_path(uri.path()).into_response()
}

fn serve_embedded_path(path: &str) -> Response {
    let normalized = normalize_path(path);
    let candidates = candidate_paths(&normalized);

    for candidate in candidates {
        if let Some(response) = embedded_file_response(&candidate) {
            return response;
        }
    }

    if has_file_extension(&normalized) {
        return StatusCode::NOT_FOUND.into_response();
    }

    embedded_file_response("index.html").unwrap_or_else(|| StatusCode::NOT_FOUND.into_response())
}

fn embedded_file_response(path: &str) -> Option<Response> {
    let file = Asset::get(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Some(
        (
            [(header::CONTENT_TYPE, mime.as_ref())],
            Body::from(file.data.into_owned()),
        )
            .into_response(),
    )
}

fn candidate_paths(path: &str) -> Vec<String> {
    if path.is_empty() {
        return vec!["index.html".to_string()];
    }

    let mut candidates = vec![path.to_string()];
    if !has_file_extension(path) {
        candidates.push(format!("{path}.html"));
        candidates.push(format!("{path}/index.html"));
    }
    candidates
}

fn normalize_path(path: &str) -> String {
    path.trim_start_matches('/')
        .trim_end_matches('/')
        .to_string()
}

fn has_file_extension(path: &str) -> bool {
    path.rsplit_once('/')
        .map_or(path, |(_, file_name)| file_name)
        .contains('.')
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;

    use super::*;

    async fn read_body(response: Response) -> String {
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        String::from_utf8(body.to_vec()).expect("body is utf8")
    }

    #[tokio::test]
    async fn serves_embedded_index() {
        let response = serve_embedded_path("/");
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        let body = read_body(response).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(content_type.as_deref(), Some("text/html"));
        assert!(body.contains("<!DOCTYPE html>"));
    }

    #[tokio::test]
    async fn serves_exported_route_html() {
        let response = serve_embedded_path("/auth/login");
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        let body = read_body(response).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(content_type.as_deref(), Some("text/html"));
        assert!(body.contains("<!DOCTYPE html>"));
    }

    #[tokio::test]
    async fn missing_asset_with_extension_returns_404() {
        let response = serve_embedded_path("/missing.css");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn extensionless_frontend_route_falls_back_to_index() {
        let response = serve_embedded_path("/client-side-route");
        let status = response.status();
        let body = read_body(response).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("<!DOCTYPE html>"));
    }
}
