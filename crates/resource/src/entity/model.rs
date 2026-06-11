#[derive(Debug, Clone)]
pub struct ResourceQuery {
    /// title模糊关键字
    pub keywords: Option<Vec<String>>,
    /// 开始时间戳
    pub start_at: Option<i64>,
    /// 结束时间戳
    pub end_at: Option<i64>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct ResourceBaseData {
    pub title: String,
    pub match_title: String,
    pub url: String,
    pub info_hash: [u8; 20],
    pub published_at: i64,
}

#[derive(Debug, Clone)]
pub struct ResourceProp {
    pub data: ResourceBaseData,
}
