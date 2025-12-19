use std::{fmt::Write, path::Path, sync::Arc, time::Duration};

use anna::{
    anime::tracker::{AnimeInfo, AnimeTracker},
    qbit::qbitorrent::Qbit,
    rss::client::Client,
};
use anyhow::{Context, Error};
use base32::Alphabet;
use chrono::{DateTime, NaiveDate};
use formatx::formatx;
use regex::Regex;
use reqwest::Url;
use sha1::{Digest, Sha1};
use tokio::{select, sync::Mutex, time};

use model::{
    anime::AnimeStatus,
    rss::{AnimeRssRecord, RssItem, RssRecord},
    torrent::Torrent,
};
use provider::db::{AnimeProvider, RssProvider, RuleProvider, ServiceConfigProvider};

#[derive(Debug, Clone)]
pub struct RuleRegex {
    pub name: String,
    pub cost: usize,
    pub re_str: String,
    pub re: Regex,
}

#[derive(Clone)]
pub struct Tasker {
    rss_db: RssProvider,
    rss_http_client: Arc<Client>,
    anime_db: AnimeProvider,
    anime: Arc<AnimeTracker>,
    rule_db: RuleProvider,
    config_db: ServiceConfigProvider,
    qbit_client: Arc<Mutex<Qbit>>,

    rules_re: Arc<Mutex<Vec<RuleRegex>>>,
}

impl Tasker {
    pub fn new(
        rss: RssProvider,
        rss_http_client: Arc<Client>,
        anime_db: AnimeProvider,
        anime: Arc<AnimeTracker>,
        rule_db: RuleProvider,
        config_db: ServiceConfigProvider,
    ) -> Self {
        Tasker {
            rss_db: rss,
            rss_http_client,
            anime_db,
            anime,
            rule_db,
            rules_re: Arc::new(Mutex::new(Vec::new())),
            qbit_client: Arc::new(Mutex::new(Qbit::new(
                "".to_string(),
                "".to_string(),
                "".to_string(),
            ))),
            config_db,
        }
    }
    pub async fn run(&self) {
        // BGM番剧更新列表获取间隔12小时
        let mut sync_calender_ticker = time::interval(Duration::from_secs(12 * 60 * 60));
        // RSS轮询间隔5分钟
        let mut check_update_ticker = time::interval(Duration::from_secs(5 * 60));

        loop {
            let s = self.clone();
            select! {
                        _ = sync_calender_ticker.tick() => {
                            tokio::spawn( async move {
                                if let Err(err) = s.update_calender().await {
                                tracing::error!("{}", err);
                                }
                            });
                        }
                        _ = check_update_ticker.tick() => {
                            tokio::spawn( async move {
                                tracing::info!("Task cycle started");
                                if let Err(e) = s.load_rules().await {
                                    tracing::error!("load_rules failed: {}", e);
                                }
                                if let Err(err) = s.update_anime_rss_record().await {
                                    tracing::error!("{}", err);
                                }
                                if let Err(err) = s.check_new_rss().await {
                                    tracing::error!("{}", err);
                                }
                                if let Err(err) = s.search_animes().await {
                                    tracing::error!("search_animes failed: {}", err);
                                }
                                tracing::info!("Task cycle finished");
                            });
                        }
            }
        }
    }

    pub async fn load_rules(&self) -> anyhow::Result<()> {
        if let Ok(Some(rules)) = self.rule_db.get_all_rules().await {
            let mut rules_re = Vec::new();
            for rule in rules {
                if let Ok(re) = Regex::new(&rule.re) {
                    rules_re.push(RuleRegex {
                        name: rule.name,
                        cost: rule.cost,
                        re_str: rule.re,
                        re,
                    });
                }
            }
            rules_re.sort_by(|a, b| a.cost.cmp(&b.cost));
            let mut lock = self.rules_re.lock().await;
            *lock = rules_re;
        }
        Ok(())
    }

    pub async fn search_animes(&self) -> anyhow::Result<()> {
        // 1. 获取目标番剧：从数据库获取所有 is_search = true 且未完结的番剧
        let all_animes = self.anime_db.get_calenders().await?;
        if all_animes.is_none() {
            return Ok(());
        }
        let mut target_animes = Vec::new();
        for anime in all_animes.unwrap() {
            if anime.status && anime.is_search {
                target_animes.push(anime);
            }
        }

        if target_animes.is_empty() {
            return Ok(());
        }

        // 2. 立即关闭搜索开关并写入数据库
        for anime in &mut target_animes {
            let mut no_search = anime.clone();
            no_search.is_search = false;
            if let Err(e) = self.anime_db.set_calender(no_search).await {
                tracing::error!("search_animes set anime no search failed, error: {}", e);
            }
            // 保持 target_animes 中的 is_search 为 true (或者不重要，因为我们已经拿到了内存对象)，
            // 但重要的是我们用这个对象去搜索。
        }

        // 3. 准备资源：获取 RSS 源列表
        let rss_list = self
            .rss_db
            .get_all_rss()
            .await
            .map_err(|e| anyhow::Error::msg(format!("search_animes get_all_rss failed, {}", e)))?
            .ok_or(Error::msg("rss list is empty"))?;

        // 4. 遍历番剧处理
        for mut anime in target_animes {
            tracing::info!("Processing search for anime: {}", anime.anime_info.name);

            let names = anime.anime_info.names();
            if names.is_empty() {
                continue;
            }

            // 阶段 A：本地历史回扫
            match self.rss_db.search_rss_records_by_keywords(&names).await {
                Ok(records) => {
                    for record in records {
                        let pub_date = record.pub_date.map(|ts| {
                            if let Some(dt) = DateTime::from_timestamp(ts, 0) {
                                dt.to_rfc2822()
                            } else {
                                "".to_string()
                            }
                        });

                        let item = RssItem {
                            title: record.title,
                            magnet: record.magnet,
                            pub_date,
                            rule_name: record.source.unwrap_or_default(),
                        };
                        self.check_anime_rules(item, &mut anime).await;
                    }
                }
                Err(e) => {
                    tracing::error!("search_animes search_rss_records_by_keywords failed: {}", e);
                }
            }

            // 阶段 B：状态检查
            // 刷新番剧状态。若 progress >= eps (已满)，break 当前番剧循环
            if let Ok(Some(latest_anime)) = self.anime_db.get_calender(anime.anime_info.id).await {
                if latest_anime.progress >= latest_anime.anime_info.eps as usize {
                    tracing::info!(
                        "Anime {} download completed during local search, skipping network search.",
                        anime.anime_info.name
                    );
                    continue;
                }
                // 更新内存中的 anime 对象，确保后续匹配用到最新的 progress
                anime = latest_anime;
            }

            // 阶段 C：网络主动搜索
            for rss in &rss_list {
                if let Some(search_url_tmpl) = &rss.search_url {
                    for name in &names {
                        let search_url = match formatx!(search_url_tmpl, name) {
                            Ok(u) => u,
                            Err(e) => {
                                tracing::error!("Format search url failed: {}", e);
                                continue;
                            }
                        };

                        tracing::debug!(
                            "Searching network for anime: {} from source: {} (keyword: {})",
                            anime.anime_info.name,
                            rss.title,
                            name
                        );

                        let r = self.rss_http_client.get_channel(&search_url).await;
                        if r.is_err() {
                            tracing::error!(
                                "search_animes get data from {} failed, {}",
                                &search_url,
                                r.unwrap_err()
                            );
                            continue;
                        }

                        let rsp = r.unwrap();
                        for i in rsp.items.iter() {
                            if i.title.is_none() {
                                continue;
                            }
                            if let Some(title) = &i.title {
                                if title.contains("合集") {
                                    continue;
                                }
                            }
                            if (i.enclosure().is_none() && i.link().is_none())
                                || i.pub_date.is_none()
                            {
                                continue;
                            }

                            let url = if let Some(e) = i.enclosure() {
                                e.url()
                            } else {
                                i.link().unwrap()
                            };
                            let title = i.title.clone().unwrap();
                            let pub_date_str = i.pub_date.clone();

                            // 入库 & 匹配
                            // 注意：这里是同步等待 hash 计算和入库，因为我们需要立即用来匹配
                            let info_hash = if let Ok(Some(record)) =
                                self.rss_db.get_rss_record_by_url(url).await
                            {
                                Some(record.info_hash)
                            } else {
                                (Tasker::get_info_hash(url).await).ok()
                            };

                            if let Some(hash) = info_hash {
                                let parsed_pub_date = pub_date_str
                                    .clone()
                                    .and_then(|date_str| Tasker::parse_pub_date(&date_str));

                                let rr = RssRecord {
                                    title: title.clone(),
                                    magnet: url.to_string(),
                                    info_hash: hash.clone(),
                                    pub_date: parsed_pub_date,
                                    source: rss.title.clone().into(), // Use RSS source title
                                    info: None,
                                    url: Some(url.to_string()),
                                };

                                if let Err(e) = self.rss_db.insert_or_update_rss_record(&rr).await {
                                    tracing::error!("search_animes insert record failed: {}", e);
                                }

                                let item = RssItem {
                                    title,
                                    magnet: url.to_string(),
                                    pub_date: pub_date_str,
                                    rule_name: rss.title.clone(),
                                };
                                self.check_anime_rules(item, &mut anime).await;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    // 更新番剧列表，写入数据库
    pub async fn update_calender(&self) -> anyhow::Result<()> {
        tracing::info!("start sync bgm calender");
        let anime =
            self.anime.get_calender().await.map_err(|e| {
                anyhow::Error::msg(format!("sync_calender get_calender failed. {}", e))
            })?;
        self.anime_db
            .set_calenders(anime)
            .await
            .map_err(|e| anyhow::Error::msg(format!("sync_calender set failed, {}", e)))
    }

    fn parse_pub_date(date_str: &str) -> Option<i64> {
        // Try parsing with RFC2822
        if let Ok(dt) = chrono::DateTime::parse_from_rfc2822(date_str) {
            let fixed_offset_8 = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
            return Some(dt.with_timezone(&fixed_offset_8).timestamp());
        }
        // Try parsing with RFC3339
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_str) {
            let fixed_offset_8 = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
            return Some(dt.with_timezone(&fixed_offset_8).timestamp());
        }
        tracing::warn!("Failed to parse pub_date string: {}", date_str);
        None
    }

    pub async fn check_new_rss(&self) -> anyhow::Result<()> {
        let records =
            self.rss_db.select_latest_rss_records().await.map_err(|e| {
                anyhow::Error::msg(format!("check_new_rss get records failed, {}", e))
            })?;

        let mut animes = self
            .anime_db
            .get_calenders()
            .await
            .map_err(|e| anyhow::Error::msg(format!("check_new_rss get animes failed, {}", e)))?
            .unwrap_or_default();

        for record in records {
            let pub_date = record.pub_date.map(|ts| {
                if let Some(dt) = DateTime::from_timestamp(ts, 0) {
                    dt.to_rfc2822()
                } else {
                    "".to_string()
                }
            });

            let item = RssItem {
                title: record.title,
                magnet: record.magnet,
                pub_date,
                rule_name: record.source.unwrap_or_default(),
            };

            for anime in animes.iter_mut() {
                self.check_anime_rules(item.clone(), anime).await;
            }
        }
        Ok(())
    }

    pub async fn update_anime_rss_record(&self) -> anyhow::Result<()> {
        // 获取RSS更新列表
        let rss_list = self
            .rss_db
            .get_all_rss()
            .await
            .map_err(|e| anyhow::Error::msg(format!("check_update get_all_rules failed, {}", e)))?
            .ok_or(Error::msg("rss list is empty"))?;
        for item in rss_list.iter() {
            tracing::debug!("check_update get rss: {:?}", item);
            if let Some(url) = item.url.clone() {
                let r = self.rss_http_client.get_channel(&url).await;
                if r.is_err() {
                    tracing::error!(
                        "check_update get data from {} failed, {}",
                        &url,
                        r.unwrap_err()
                    );
                    continue;
                }
                let rsp = r.unwrap();
                for i in rsp.items.iter() {
                    if i.title.is_none() {
                        continue;
                    }

                    // 跳过合集
                    if let Some(title) = &i.title {
                        if title.contains("合集") {
                            continue;
                        }
                    }

                    if (i.enclosure().is_none() && i.link().is_none()) || i.pub_date.is_none() {
                        continue;
                    }

                    let url = if let Some(e) = i.enclosure() {
                        e.url()
                    } else {
                        i.link().unwrap()
                    };
                    let title = i.title.clone().unwrap();

                    let s = self.clone();
                    let url = url.to_string();
                    let item_title = item.title.clone();
                    let pub_date = i.pub_date.clone();

                    tokio::spawn(async move {
                        let info_hash =
                            if let Ok(Some(record)) = s.rss_db.get_rss_record_by_url(&url).await {
                                tracing::debug!(
                                "Found existing RSS record for URL: '{}', using cached hash: '{}'",
                                url,
                                record.info_hash
                            );
                                Some(record.info_hash)
                            } else {
                                (Tasker::get_info_hash(&url).await).ok()
                            };

                        if let Some(hash) = info_hash {
                            tracing::debug!(
                                "Processing RSS item: [{}] Title: '{}', Hash: '{}', PubDate: '{:?}'",
                                item_title,
                                title,
                                hash,
                                pub_date
                            );

                            let parsed_pub_date = pub_date.clone().and_then(|date_str| {
                                let d = Tasker::parse_pub_date(&date_str);
                                if d.is_none() {
                                    tracing::warn!(
                                        "Failed to parse date for item: '{}', DateStr: '{}'",
                                        title,
                                        date_str
                                    );
                                }
                                d
                            });

                            let rr = RssRecord {
                                title: title.clone(),
                                magnet: url.to_string(),
                                info_hash: hash.clone(),
                                pub_date: parsed_pub_date,
                                source: Some(item_title.clone()),
                                info: None,
                                url: Some(url.clone()),
                            };

                            match s.rss_db.insert_or_update_rss_record(&rr).await {
                                Ok(_) => {}
                                Err(e) => tracing::error!(
                                    "Failed to save RSS record: [{}] '{}' (Hash: {}). Error: {}",
                                    item_title,
                                    title,
                                    hash,
                                    e
                                ),
                            }
                        } else {
                            tracing::warn!(
                                "Failed to calculate hash for RSS item: [{}] '{}', URL: '{}'",
                                item_title,
                                title,
                                url
                            );
                        }
                    });
                }
            }
        }
        Ok(())
    }

    async fn check_anime_rules(&self, msg: RssItem, anime_status: &mut AnimeStatus) {
        if let Ok(Some(anime)) = self.anime_db.get_calender(anime_status.anime_info.id).await {
            anime_status.anime_info = anime.anime_info;
        }
        let rules = self.rules_re.lock().await;
        if !rules.is_empty() {
            let anime = &anime_status.anime_info;
            for name in anime.names().iter() {
                if msg.title.contains(name) {
                    // 遍历正则规则，匹配标题
                    let mut is_matched = false;
                    let mut matched_rule_name = String::new();
                    for rule in rules.iter() {
                        if rule.re.is_match(&msg.title) {
                            tracing::debug!(
                                "check_anime_rules match rule: {} cost: {} title: {}",
                                &rule.name,
                                rule.cost,
                                &msg.title
                            );
                            is_matched = true;
                            matched_rule_name = rule.name.clone();
                            break;
                        }
                    }
                    if !is_matched {
                        continue;
                    }

                    // 判断当前种子的上传时间是否大于该番剧季度的开始更新时间
                    if let Some(pub_date) = &msg.pub_date {
                        if let Ok(pub_date) = DateTime::parse_from_rfc2822(pub_date) {
                            if let Ok(date) = NaiveDate::parse_from_str(
                                &anime_status.anime_info.air_date,
                                "%Y-%m-%d",
                            ) {
                                if pub_date
                                    .date_naive()
                                    // 兼容一周加一天的误差，防止第一集提前放映无法通过检查
                                    .checked_add_days(chrono::Days::new(8))
                                    .unwrap_or(pub_date.date_naive())
                                    < date
                                {
                                    tracing::debug!("check_anime_rules check {} success, pub_date < date, skip, pub_date: {:?}, bgm_date: {}",&msg.title,&msg.pub_date,&anime_status.anime_info.air_date);
                                    continue;
                                }
                            }
                        }
                    }
                    // 判断是否已经命中过规则
                    if anime_status.rule_name.is_empty() {
                        anime_status.rule_name = matched_rule_name.clone();
                        if let Err(e) = self.anime_db.set_calender(anime_status.clone()).await {
                            tracing::error!("check_anime_rules set set_calender failed, {}", e);
                            continue;
                        }
                    }

                    if !anime_status.rule_name.eq(&matched_rule_name) {
                        tracing::debug!(
                            "check_anime_rules rule mismatch, anime rule: {}, matched rule: {}",
                            anime_status.rule_name,
                            matched_rule_name
                        );
                        continue;
                    }

                    tracing::debug!(
                        "check_anime_rules anime: {} bt: {} rule: {}",
                        &anime.name,
                        &msg.title,
                        &matched_rule_name,
                    );
                    self.handle_rss(&matched_rule_name, msg, anime_status).await;
                    return;
                }
            }
        }
    }

    async fn handle_rss(&self, rule_name: &str, msg: RssItem, anime_status: &AnimeStatus) {
        let s = self.clone();
        let rule_name = rule_name.to_string();
        let anime = anime_status.anime_info.clone();

        if let Ok(info_hash) = Tasker::get_info_hash(&msg.magnet).await {
            if let Ok(None) = s.anime_db.get_anime_record(anime.id, &info_hash).await {
                // TODO:
                // 发送磁力链接到qbit下载，设置下载路径
                // 考虑是否直接使用qbit的命名功能，这个功能曾经不稳定，接口返回ok但实际没有命名成功
                if let Err(e) = s.send_qbit(&msg.magnet, &anime, &info_hash).await {
                    tracing::error!("handle_rss send {:?} to qbit failed, error: {}", &msg, e);
                    return;
                }
                tracing::info!(
                    "handle_rss download anime: {} bt: {}",
                    &anime.name,
                    &msg.title
                );

                if let Err(e) = s
                    .anime_db
                    .set_anime_recode(
                        anime.id,
                        AnimeRssRecord {
                            anime_id: anime.id,
                            title: msg.title,
                            magnet: msg.magnet,
                            rule_name: rule_name.to_string(),
                            info_hash: info_hash.clone(),
                            created_time: None,
                        },
                    )
                    .await
                {
                    tracing::error!("handle_rss set_anime_recode failed, error: {}", e);
                }
            }
            // 检查是否已经完结
            // 完结则修改状态为false，退出监听
            if let Ok(progress) = s.get_update_progress(&anime).await {
                match s.anime_db.get_calender(anime.id).await {
                    Ok(status) => {
                        if let Some(mut status) = status {
                            if progress >= status.anime_info.eps as usize {
                                status.status = false;
                                status.progress = progress;
                                if let Err(e) = s.anime_db.set_calender(status).await {
                                    tracing::error!(
                                        "handle_rss season over set anime status failed, {}",
                                        e
                                    );
                                }
                            } else if progress > status.progress {
                                status.progress = progress;
                                if let Err(e) = s.anime_db.set_calender(status).await {
                                    tracing::error!(
                                        "handle_rss season update progress set anime status failed, {}",
                                        e
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("handle_rss season over get anime status failed, {}", e)
                    }
                }
            }
        }
    }

    /// send_qbit
    ///
    /// 发送下载链接到qbit下载
    ///
    /// 下载路径：{config_path}/{anime.name}/S{02:anime.season}
    ///
    /// 当qbit_config和download_path都为空时，不送发任何信息
    async fn send_qbit(&self, url: &str, anime: &AnimeInfo, hash: &str) -> Result<(), Error> {
        let mut client = self.qbit_client.lock().await;
        let qbit_config = self
            .config_db
            .get_qbit()
            .await?
            .ok_or(Error::msg("send_qbit get qbit config empty"))?;
        let download_path = self
            .config_db
            .get_path()
            .await?
            .ok_or(Error::msg("send_qbit get download path empty"))?;

        client.load_new_config(&qbit_config).await?;
        client.check_and_login().await?;
        let download_path =
            Path::new(&download_path).join(format!("{}/S{:02}", anime.search_name, anime.season));
        client
            .add(
                url,
                download_path
                    .to_str()
                    .ok_or(Error::msg("send_qbit get download path failed"))?,
                hash,
            )
            .await?;

        Ok(())
    }

    // 检查是否完结，返回更新进度百分比
    async fn get_update_progress(&self, anime: &AnimeInfo) -> Result<usize, Error> {
        let anime_list = self
            .anime_db
            // 获取番剧的下载记录
            .get_anime_rss_recodes(anime.id)
            .await?
            .ok_or(Error::msg("not found anime records"))?;
        let eps = Self::get_season_eps(anime_list)?;
        tracing::debug!("check_season_over anime {} eps {:?}", &anime.name, &eps);
        Ok(eps.len())
    }

    pub fn get_season_eps(anime_list: Vec<AnimeRssRecord>) -> Result<Vec<i64>, Error> {
        let anime_list: Vec<Vec<f64>> = anime_list
            .iter()
            // 将标题的数字获取出来转成浮点数数组，并过滤掉小数，只保留整数
            .map(|anime| {
                Regex::new(r"\d+(\.\d+)?")
                    .unwrap()
                    .captures_iter(&anime.title)
                    .filter_map(|cap| cap[0].parse::<f64>().ok())
                    .filter(|i| i.eq(&i.trunc()))
                    .collect::<Vec<f64>>()
            })
            .collect();
        // 如果下载记录只有两条以下，直接获取第一个数字返回
        if anime_list.len() <= 2 {
            return Ok(anime_list
                .iter()
                .map(|i| *(i.first().unwrap_or(&0.0)) as i64)
                .collect());
        }
        let mut eps = Vec::new();
        // 遍历数组的下标，最大下标为长度最短的数组长度
        for index in 0..anime_list.iter().map(|v| v.len()).min().unwrap_or(0) {
            // 将每个数组转化为相同下标的一列数组
            let mut i_eps = Vec::new();
            for i in anime_list.iter() {
                if i.len() > index {
                    i_eps.push(i[index] as i64);
                }
            }

            // 过滤掉重复数字出现三次的一列数组
            if !i_eps
                .iter()
                .fold(std::collections::HashMap::new(), |mut acc, &x| {
                    *acc.entry(x).or_insert(0) += 1;
                    acc
                })
                .values()
                .all(|&count| count <= 2)
            {
                continue;
            }
            // 从做开始的第一个不重复三次的数字就是番剧的集数，退出循环
            eps = i_eps;
            break;
        }
        eps.sort();
        eps.dedup();
        // 去掉第0集
        Ok(eps.into_iter().filter(|&x| x > 0).collect())
    }

    pub async fn get_info_hash(url: &str) -> Result<String, Error> {
        if let Some(hash_info) = Self::get_magnet_info_hash(url) {
            if hash_info.len() <= 32 {
                Ok(
                    base32::decode(Alphabet::Rfc4648 { padding: true }, &hash_info)
                        .context("base32 to hex failed")?
                        .iter()
                        .fold(String::new(), |mut acc, byte| {
                            write!(&mut acc, "{:02x}", byte).unwrap();
                            acc
                        }),
                )
            } else {
                Ok(hash_info)
            }
        } else {
            let bytes = reqwest::get(url).await?.bytes().await?;
            let info: Torrent = serde_bencode::from_bytes(&bytes)?;
            let mut hasher = Sha1::new();
            let info = serde_bencode::to_bytes(&info.info)?;
            hasher.update(info);
            let info_hash = format!("{:x}", hasher.finalize());
            Ok(info_hash.to_lowercase())
        }
    }

    fn get_magnet_info_hash(magnet_link: &str) -> Option<String> {
        let url = Url::parse(magnet_link).ok()?;
        let xt_param = url.query_pairs().find(|(k, _)| k == "xt")?;
        let info_hash = xt_param.1.strip_prefix("urn:btih:")?;

        Some(info_hash.to_string())
    }
}
