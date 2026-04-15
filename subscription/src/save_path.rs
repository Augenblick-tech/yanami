use domain::anime::AnimeMetadata;

pub fn build_relative_save_path(metadata: &AnimeMetadata) -> String {
    format!(
        "{}/S{:02}",
        sanitize_path_component(&metadata.series_name()),
        metadata.season
    )
}

fn sanitize_path_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|char| match char {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => char,
        })
        .collect::<String>();
    sanitized.trim().trim_matches('.').to_string()
}
