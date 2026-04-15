use domain::anime::AnimeMetadata;

pub fn subscription_keywords(metadata: &AnimeMetadata) -> Vec<String> {
    let mut keywords = metadata.search_keywords();
    keywords.sort();
    keywords.dedup();
    keywords
}
