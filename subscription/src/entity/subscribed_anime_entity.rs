#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchState {
    Stopped,
    Pending,
    Running,
    LocalMatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubscribedAnimeState {
    Active,
    Stop,
    Completed,
}

#[derive(Debug, Clone)]
pub struct SubscribedAnimeEntity {
    id: u32,
    space_id: u32,
    anime_id: u32,
    state: SubscribedAnimeState,
    bound_rule_name: Option<String>,
    search_state: SearchState,
    progress: u32,
}

impl SubscribedAnimeEntity {
    pub(crate) fn new(
        id: u32,
        anime_id: u32,
        space_id: u32,
        state: SubscribedAnimeState,
        bound_rule_name: Option<String>,
        search_state: SearchState,
        progress: u32,
    ) -> Self {
        Self {
            id,
            space_id,
            anime_id,
            state,
            bound_rule_name,
            search_state,
            progress,
        }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn anime_id(&self) -> u32 {
        self.anime_id
    }

    pub fn space_id(&self) -> u32 {
        self.space_id
    }
}
