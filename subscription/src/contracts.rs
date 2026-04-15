#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentMatchStats {
    pub resource_count: usize,
    pub matched_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchStats {
    pub searched_subscription_count: usize,
    pub saved_count: usize,
    pub matched_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingEpisodeStats {
    pub checked_subscription_count: usize,
    pub resumed_anime_count: usize,
}
