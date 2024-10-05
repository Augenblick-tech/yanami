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

use crate::{
    models::{
        anime::AnimeStatus,
        rss::{AnimeRssRecord, RssItem},
        rule::GroupRule,
        torrent::Torrent,
    },
    provider::db::db_provider::{AnimeProvider, RssProvider, RuleProvider, ServiceConfigProvider},
};

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AnimeTask {
    pub info: AnimeInfo,
    pub is_canncel: bool,
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
        let (ab, _) = broadcast::channel::<AnimeTask>(10);
        let (arb, _) = broadcast::channel::<RssItem>(10);
        Tasker {
            rss_db: rss,
            rss_http_client,
            anime_db,
            anime,
            rule_db,
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
                    if s.rss_send_map.lock().await.len() > 0 {
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
        let (tx, mut recv) = mpsc::channel(20);
        let mut broadcast_recv = self.anime_rss_broadcast.subscribe();
        send_map.insert(anime_status.anime_info.id, tx);
        let s = self.clone();
        let mut anime = anime_status.clone();
        spawn(async move {
            tracing::debug!("spawn anime: {:?}", &anime);
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
            .map_err(|e| anyhow::Error::msg(format!("check_update get_all_rules failed, {}", e)))?
            .ok_or(Error::msg("rss list is empty"))?;
        let anime_list = self
            .anime_db
            .get_calenders()
            .map_err(|e| anyhow::Error::msg(format!("check_update get_calender failed, {}", e)))?
            .ok_or(Error::msg("anime list is empty"))?;
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
                    if i.enclosure().is_none() && i.link().is_none() {
                        continue;
                    }

                    let url = if let Some(e) = i.enclosure() {
                        e.url()
                    } else {
                        i.link().unwrap()
                    };

                    let ri = RssItem {
                        title: i.title.clone().unwrap(),
                        magnet: url.to_string(),
                        pub_date: i.pub_date.clone(),
                    };
                    if let Err(err) = self.anime_rss_broadcast.send(ri.clone()) {
                        tracing::error!("broadcast rss item to chan failed, {}", err);
                    }
                }
            }
            // 特定番剧的搜索RSS则只给该番发送
            if let Some(search_url) = item.search_url.clone() {
                for anime in anime_list.iter() {
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
                            spawn(async move {
                                if let Ok(rsp) = rss_http_client.get_channel(&url).await {
                                    for item in rsp.items.iter() {
                                        if item.enclosure().is_none() && item.link().is_none() {
                                            continue;
                                        }

                                        let url = if let Some(e) = item.enclosure() {
                                            e.url()
                                        } else {
                                            item.link().unwrap()
                                        };
                                        // 该错误只会在chan被关闭时抛出，而接收端未启动时chan就是处于关闭状态的，此时允许写入失败，忽略错误
                                        let _ = chan
                                            .send(RssItem {
                                                title: item.title.clone().unwrap(),
                                                magnet: url.to_string(),
                                                pub_date: item.pub_date.clone(),
                                            })
                                            .await;
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
        let anime =
            self.anime.get_calender().await.map_err(|e| {
                anyhow::Error::msg(format!("sync_calender get_calender failed. {}", e))
            })?;
        for i in anime.iter() {
            // 番剧已经被设置为不追踪，则跳过监听
            if let Some(anime_status) = self.anime_db.get_calender(i.id)? {
                if !anime_status.status {
                    continue;
                }
            }
            if let Err(e) = self
                .start_listener(AnimeStatus {
                    status: true,
                    rule_name: "".to_string(),
                    anime_info: i.clone(),
                })
                .await
            {
                tracing::error!("sync_calender start_listener failed, error: {}", e);
            }
        }
        self.anime_db
            .set_calenders(anime)
            .map_err(|e| anyhow::Error::msg(format!("sync_calender set failed, {}", e)))
    }

    async fn check_anime_rules(&self, msg: RssItem, anime_status: &mut AnimeStatus) {
        if let Ok(Some(anime)) = self.anime_db.get_calender(anime_status.anime_info.id) {
            anime_status.anime_info = anime.anime_info;
        }
        if let Ok(Some(rules)) = self.rule_db.get_all_rules() {
            let anime = &anime_status.anime_info;
            for rule in rules.iter() {
                for i in rule.rules.iter() {
                    for name in anime.names().iter() {
                        let name = formatx!(&i.re, name);
                        if name.is_err() {
                            tracing::error!(
                                "check_anime_rules format re failed, error: {}",
                                name.unwrap_err()
                            );
                            continue;
                        }
                        let re = name.unwrap();
                        if Regex::new(&re).unwrap().is_match(&msg.title) {
                            // 记录下载的内容到数据库
                            tracing::debug!(
                                "anime: {}\nbt: {}\nrule: {}",
                                &anime.name,
                                &msg.title,
                                &i.re
                            );
                            self.handle_rss(rule, msg, anime_status).await;
                            return;
                        }
                    }
                }
            }
        }
    }

    async fn handle_rss(&self, rule: &GroupRule, msg: RssItem, anime_status: &mut AnimeStatus) {
        // 判断是否已经命中过规则
        if anime_status.rule_name.is_empty() {
            anime_status.rule_name = rule.name.clone();
            if let Err(e) = self.anime_db.set_calender(anime_status.clone()) {
                tracing::error!("handle_rss set set_calender failed, {}", e);
                return;
            }
        }

        if !anime_status.rule_name.eq(&rule.name) {
            return;
        }

        // 判断当前种子的上传时间是否大于该番剧季度的开始更新时间
        if let Some(pub_date) = &msg.pub_date {
            if let Ok(pub_date) = DateTime::parse_from_rfc2822(pub_date) {
                if let Ok(date) =
                    NaiveDate::parse_from_str(&anime_status.anime_info.air_date, "%Y-%m-%d")
                {
                    if pub_date.date_naive() < date {
                        tracing::debug!(
                            "handle_rss check {} success, pub_date < date, skip, pub_date: {:?}, bgm_date: {}",
                            &anime_status.anime_info.name,
                            &msg.pub_date,
                            &anime_status.anime_info.air_date
                        );
                        return;
                    }
                } else {
                    tracing::debug!(
                        "handle_rss check {} bgm date failed, pub_date: {:?}, bgm_date: {}",
                        &anime_status.anime_info.name,
                        &msg.pub_date,
                        &anime_status.anime_info.air_date
                    );
                }
            } else {
                tracing::debug!(
                    "handle_rss check {} pub date failed, pub_date: {:?}, bgm_date: {}",
                    &anime_status.anime_info.name,
                    &msg.pub_date,
                    &anime_status.anime_info.air_date
                );
            }
        } else {
            tracing::debug!(
                "handle_rss check {} pub date not found, pub_date: {:?}, bgm_date: {}",
                &anime_status.anime_info.name,
                &msg.pub_date,
                &anime_status.anime_info.air_date
            );
        }

        let anime = &anime_status.anime_info;
        if let Ok(info_hash) = Self::get_info_hash(&msg.magnet).await {
            let r = self.anime_db.get_anime_record(anime.id, &info_hash);
            if r.is_err() {
                return;
            }
            if r.unwrap().is_none() {
                // TODO:
                // 发送磁力链接到qbit下载，设置下载路径
                // 考虑是否直接使用qbit的命名功能，这个功能曾经不稳定，接口返回ok但实际没有命名成功
                if let Err(e) = self.send_qbit(&msg.magnet, anime).await {
                    tracing::error!(
                        "check_anime_rules send {:?} to qbit failed, error: {}",
                        &msg,
                        e
                    );
                    return;
                }

                if let Err(e) = self.anime_db.set_anime_recode(
                    anime.id,
                    AnimeRssRecord {
                        title: msg.title,
                        magnet: msg.magnet,
                        rule_name: rule.name.clone(),
                        info_hash,
                    },
                ) {
                    tracing::error!("check_anime_rules set_calender failed, error: {}", e);
                }
            }
            // 检查是否已经完结
            // 完结则修改状态为false，退出监听
            if let Ok(is_season_over) = self.check_season_over(anime) {
                if is_season_over {
                    match self.anime_db.get_calender(anime.id) {
                        Ok(status) => {
                            if let Some(mut status) = status {
                                status.status = false;
                                if let Err(e) = self.anime_db.set_calender(status) {
                                    tracing::error!(
                                        "check_anime_rules season over set anime status failed, {}",
                                        e
                                    );
                                }
                            }
                        }
                        Err(e) => tracing::error!(
                            "check_anime_rules season over get anime status failed, {}",
                            e
                        ),
                    }
                    if let Err(e) = self.anime_broadcast.send(AnimeTask {
                        info: anime.clone(),
                        is_canncel: true,
                    }) {
                        tracing::error!(
                            "{} is season update over, stop listen failed, error: {}",
                            anime.name,
                            e
                        );
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
    async fn send_qbit(&self, url: &str, anime: &AnimeInfo) -> Result<(), Error> {
        let mut client = self.qbit_client.lock().await;
        let qbit_config = self
            .config_db
            .get_qbit()?
            .ok_or(Error::msg("send_qbit get qbit config empty"))?;
        let download_path = self
            .config_db
            .get_path()?
            .ok_or(Error::msg("send_qbit get download path empty"))?;

        client.load_new_config(&qbit_config).await?;
        let download_path =
            Path::new(&download_path).join(format!("{}/S{:02}", anime.name, anime.season));
        client
            .add(
                url,
                download_path
                    .to_str()
                    .ok_or(Error::msg("send_qbit get download path failed"))?,
            )
            .await?;

        Ok(())
    }

    fn check_season_over(&self, anime: &AnimeInfo) -> Result<bool, Error> {
        let anime_list = self
            .anime_db
            // 获取番剧的下载记录
            .get_anime_rss_recodes(anime.id)?
            .ok_or(Error::msg("not found anime records"))?;
        let eps = Self::get_season_eps(anime_list)?;
        Ok(*(eps.last().unwrap_or(&0)) >= anime.eps)
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
