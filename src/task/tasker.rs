use std::{collections::HashMap, fmt::Write, path::Path, sync::Arc, time::Duration};

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
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tokio::{
    select, spawn,
    sync::{
        broadcast::{self},
        mpsc, Mutex,
    },
    time,
};

use model::{
    anime::AnimeStatus,
    rss::{AnimeRssRecord, RssItem},
    torrent::Torrent,
};
use provider::db::{AnimeProvider, RssProvider, RuleProvider, ServiceConfigProvider};

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AnimeTask {
    pub info: AnimeInfo,
    pub is_canncel: bool,
}

#[derive(Debug, Clone)]
pub struct RuleRegex {
    pub name: String,
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

    // 用于发送番剧退出广播
    anime_broadcast: broadcast::Sender<AnimeTask>,
    // 用于发送全站RSS的更新
    anime_rss_broadcast: broadcast::Sender<RssItem>,
    // 用于发送特定番剧的搜索结果
    rss_send_map: Arc<Mutex<HashMap<i64, mpsc::Sender<RssItem>>>>,
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
        let (ab, _) = broadcast::channel::<AnimeTask>(100);
        let (arb, _) = broadcast::channel::<RssItem>(100000);
        Tasker {
            rss_db: rss,
            rss_http_client,
            anime_db,
            anime,
            rule_db,
            rules_re: Arc::new(Mutex::new(Vec::new())),
            anime_broadcast: ab,
            anime_rss_broadcast: arb,
            rss_send_map: Arc::new(Mutex::new(HashMap::new())),
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

        // 启动时从数据库恢复历史监听
        if let Err(e) = self.init_anime_listener().await {
            tracing::error!("task run init_anime_listener failed, error: {}", e);
        }

        // 启动广播监听，当收到番剧季度完结时，从map记录中移除
        let send_map = self.rss_send_map.clone();
        let mut rx = self.anime_broadcast.subscribe();
        tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                let mut send_map = send_map.lock().await;
                if msg.is_canncel {
                    tracing::info!("stop worker {:?}", &msg.info);
                    send_map.remove(&msg.info.id);
                }
            }
        });

        loop {
            let s = self.clone();
            select! {
                _ = sync_calender_ticker.tick() => {
                    tokio::spawn( async move {
                        if let Err(err) = s.sync_calender().await {
                            tracing::error!("{}", err);
                        }
                    });
                }
                _ = check_update_ticker.tick() => {
                    if !s.rss_send_map.lock().await.is_empty() {
                        tokio::spawn( async move {
                            if let Err(err) = s.check_update().await {
                                tracing::error!("{}", err);
                            }
                        });
                    }
                }
            }
        }
    }

    async fn start_listener(&self, anime_status: AnimeStatus) -> Result<(), Error> {
        // for i in anime.iter() {
        let mut rx = self.anime_broadcast.subscribe();
        // let anime = i.clone();
        let mut send_map = self.rss_send_map.lock().await;
        // 如果已经启动过协程监听，则跳过
        if send_map.get(&anime_status.anime_info.id).is_some() {
            return Ok(());
        }
        let (tx, mut recv) = mpsc::channel(10000);
        let mut broadcast_recv = self.anime_rss_broadcast.subscribe();
        send_map.insert(anime_status.anime_info.id, tx);
        let s = self.clone();
        let mut anime = anime_status.clone();
        spawn(async move {
            tracing::info!("spawn anime: {:?}", &anime);
            loop {
                select! {
                    Ok(msg) = rx.recv() => {
                        if msg.is_canncel && anime_status.anime_info.id == msg.info.id {
                            return;
                        }
                    }
                    Some(msg) = recv.recv() => {
                        s.check_anime_rules(msg, &mut anime).await;
                    }
                    Ok(msg) = broadcast_recv.recv() => {
                        tracing::debug!("broadcasr recv {:?}", msg);
                        s.check_anime_rules(msg,&mut anime).await;
                    }
                }
            }
        });
        Ok(())
    }

    async fn init_anime_listener(&self) -> Result<(), Error> {
        let anime = self
            .anime_db
            .get_calenders()
            .await
            .map_err(|e| {
                anyhow::Error::msg(format!("init_anime_listener get_calender failed, {}", e))
            })?
            .ok_or(Error::msg("anime list is empty"))?;
        for i in anime.iter() {
            if i.status {
                if let Err(e) = self.start_listener(i.clone()).await {
                    tracing::error!(
                        "init_anime_listener start_listener failed, anime: {:?} error: {}",
                        i,
                        e
                    );
                }
            }
        }
        Ok(())
    }

    async fn check_update(&self) -> Result<(), Error> {
        let rss_list = self
            .rss_db
            .get_all_rss()
            .await
            .map_err(|e| anyhow::Error::msg(format!("check_update get_all_rules failed, {}", e)))?
            .ok_or(Error::msg("rss list is empty"))?;
        let anime_list = self
            .anime_db
            .get_calenders()
            .await
            .map_err(|e| anyhow::Error::msg(format!("check_update get_calender failed, {}", e)))?
            .ok_or(Error::msg("anime list is empty"))?;
        let rules = self
            .rule_db
            .get_all_rules()
            .await?
            .context("rules is empty")?;
        {
            let mut rules_re = self.rules_re.lock().await;
            rules_re.retain_mut(|item| {
                rules
                    .iter()
                    .any(|r| r.re == item.re_str && r.name == item.name)
            });
            for rule in rules.iter() {
                let re = rule.re.clone();
                if let Some(item) = rules_re.iter_mut().find(|r| r.name == rule.name) {
                    // 正则变化则修改
                    if item.re_str != rule.re {
                        if let Ok(re) = Regex::new(&re) {
                            let rss_regex = RuleRegex {
                                name: rule.name.clone(),
                                re_str: rule.re.clone(),
                                re,
                            };
                            *item = rss_regex;
                        }
                    }
                } else {
                    // 不存在该正则则插入
                    if let Ok(re) = Regex::new(&re) {
                        let rss_regex = RuleRegex {
                            name: rule.name.clone(),
                            re_str: rule.re.clone(),
                            re,
                        };
                        rules_re.push(rss_regex);
                    }
                }
            }
        }
        for item in rss_list.iter() {
            tracing::debug!("check_update get rss: {:?}", item);
            if let Some(url) = item.url.clone() {
                let r = self.rss_http_client.get_channel(&url).await;
                if r.is_err() {
                    tracing::error!(
                        "check_update get_calender {} failed, {}",
                        &item.url.clone().unwrap(),
                        r.unwrap_err()
                    );
                    continue;
                }
                let rsp = r.unwrap();
                // 全站RSS则获取后给所有番剧发送广播
                for i in rsp.items.iter() {
                    // tracing::debug!("check_update rss: {:?}", i);
                    if i.title.is_none() {
                        continue;
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
                    {
                        let rules_re = self.rules_re.lock().await;
                        for re in rules_re.iter() {
                            if re.re.is_match(&title) {
                                let ri = RssItem {
                                    title,
                                    magnet: url.to_string(),
                                    pub_date: i.pub_date.clone(),
                                    rule_name: re.name.clone(),
                                };
                                tracing::debug!("broadcast rss {:?}", ri);
                                if let Err(err) = self.anime_rss_broadcast.send(ri.clone()) {
                                    tracing::error!("broadcast rss item to chan failed, {}", err);
                                }
                                break;
                            }
                        }
                    }
                }
            }
            // 特定番剧的搜索RSS则只给该番发送
            // 该行为会导致频繁请求RSS，触发nyaa的429，故默认停用
            if let Some(search_url) = item.search_url.clone() {
                for anime in anime_list.iter().filter(|anime| anime.is_search) {
                    for url in anime.anime_info.names().iter().filter_map(|name| {
                        match formatx!(&search_url, &name) {
                            Ok(url) => Some(url),
                            Err(_) => None,
                        }
                    }) {
                        if let Some(chan) = self.rss_send_map.lock().await.get(&anime.anime_info.id)
                        {
                            let chan = chan.clone();
                            let rss_http_client = self.rss_http_client.clone();
                            tracing::debug!("check_update search_url: {}", url);
                            let s = self.clone();
                            spawn(async move {
                                if let Ok(rsp) = rss_http_client.get_channel(&url).await {
                                    for item in rsp.items.iter() {
                                        if item.title.is_none() {
                                            continue;
                                        }
                                        if (item.enclosure().is_none() && item.link().is_none())
                                            || item.pub_date.is_none()
                                        {
                                            continue;
                                        }

                                        let url = if let Some(e) = item.enclosure() {
                                            e.url()
                                        } else {
                                            item.link().unwrap()
                                        };
                                        let title = item.title.clone().unwrap();
                                        {
                                            let rules_re = s.rules_re.lock().await;
                                            for re in rules_re.iter() {
                                                if re.re.is_match(&title) {
                                                    let ri = RssItem {
                                                        title,
                                                        magnet: url.to_string(),
                                                        pub_date: item.pub_date.clone(),
                                                        rule_name: re.name.clone(),
                                                    };
                                                    // 该错误只会在chan被关闭时抛出，而接收端未启动时chan就是处于关闭状态的，此时允许写入失败，忽略错误
                                                    let _ = chan.send(ri).await;
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn sync_calender(&self) -> Result<(), Error> {
        tracing::info!("start sync bgm calender");
        let anime =
            self.anime.get_calender().await.map_err(|e| {
                anyhow::Error::msg(format!("sync_calender get_calender failed. {}", e))
            })?;
        for i in anime.iter() {
            // 番剧已经被设置为不追踪，则跳过监听
            if let Some(anime_status) = self.anime_db.get_calender(i.id).await? {
                if !anime_status.status {
                    continue;
                }
            }
            if let Err(e) = self
                .start_listener(AnimeStatus {
                    status: true,
                    rule_name: "".to_string(),
                    anime_info: i.clone(),
                    is_search: false,
                    is_lock: false,
                    progress: 0,
                })
                .await
            {
                tracing::error!("sync_calender start_listener failed, error: {}", e);
            }
        }
        self.anime_db
            .set_calenders(anime)
            .await
            .map_err(|e| anyhow::Error::msg(format!("sync_calender set failed, {}", e)))
    }

    async fn check_anime_rules(&self, msg: RssItem, anime_status: &mut AnimeStatus) {
        if let Ok(Some(anime)) = self.anime_db.get_calender(anime_status.anime_info.id).await {
            anime_status.anime_info = anime.anime_info;
        }
        if let Ok(Some(mut rules)) = self.rule_db.get_all_rules().await {
            let anime = &anime_status.anime_info;
            rules.sort_by(|a, b| a.cost.cmp(&b.cost));
            for name in anime.names().iter() {
                if msg.title.contains(name) {
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
                        anime_status.rule_name = msg.rule_name.clone();
                        if let Err(e) = self.anime_db.set_calender(anime_status.clone()).await {
                            tracing::error!("check_anime_rules set set_calender failed, {}", e);
                            continue;
                        }
                    }

                    if !anime_status.rule_name.eq(&msg.rule_name) {
                        continue;
                    }

                    tracing::debug!(
                        "check_anime_rules anime: {} bt: {} rule: {}",
                        &anime.name,
                        &msg.title,
                        &msg.rule_name,
                    );
                    self.handle_rss(&msg.rule_name.clone(), msg, anime_status)
                        .await;
                    return;
                }
            }
        }
    }

    async fn handle_rss(&self, rule_name: &str, msg: RssItem, anime_status: &AnimeStatus) {
        let anime = &anime_status.anime_info;
        if let Ok(info_hash) = Self::get_info_hash(&msg.magnet).await {
            if let Ok(None) = self.anime_db.get_anime_record(anime.id, &info_hash).await {
                // TODO:
                // 发送磁力链接到qbit下载，设置下载路径
                // 考虑是否直接使用qbit的命名功能，这个功能曾经不稳定，接口返回ok但实际没有命名成功
                if let Err(e) = self.send_qbit(&msg.magnet, anime, &info_hash).await {
                    tracing::error!("handle_rss send {:?} to qbit failed, error: {}", &msg, e);
                    return;
                }
                tracing::info!(
                    "handle_rss download anime: {} bt: {}",
                    &anime.name,
                    &msg.title
                );

                if let Err(e) = self
                    .anime_db
                    .set_anime_recode(
                        anime.id,
                        AnimeRssRecord {
                            title: msg.title,
                            magnet: msg.magnet,
                            rule_name: rule_name.to_string(),
                            info_hash,
                        },
                    )
                    .await
                {
                    tracing::error!("handle_rss set_anime_recode failed, error: {}", e);
                }
            }
            // 检查是否已经完结
            // 完结则修改状态为false，退出监听
            if let Ok(progress) = self.get_update_progress(anime).await {
                match self.anime_db.get_calender(anime.id).await {
                    Ok(status) => {
                        if let Some(mut status) = status {
                            if progress >= status.anime_info.eps as usize {
                                status.status = false;
                                status.progress = progress;
                                if let Err(e) = self.anime_db.set_calender(status).await {
                                    tracing::error!(
                                        "handle_rss season over set anime status failed, {}",
                                        e
                                    );
                                }
                                if let Err(e) = self.anime_broadcast.send(AnimeTask {
                                    info: anime.clone(),
                                    is_canncel: true,
                                }) {
                                    tracing::error!( "handle_rss {} is season update over, stop listen failed, error: {}", anime.name, e);
                                } else {
                                    tracing::info!("handle_rss stop anime {:?}", &anime);
                                }
                            } else if progress > status.progress {
                                status.progress = progress;
                                if let Err(e) = self.anime_db.set_calender(status).await {
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
        // 如果下载记录只有两条以下，则不做判断
        if anime_list.len() <= 2 {
            return Ok(Vec::new());
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
        Ok(eps)
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
