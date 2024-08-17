use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use anna::{
    anime::anime::{AnimeInfo, AnimeTracker},
    rss::rss::RssHttpClient,
};
use anyhow::Error;
use base32::Alphabet;
use formatx::formatx;
use regex::Regex;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tokio::{
    select, spawn,
    sync::{
        broadcast::{self},
        mpsc,
    },
    time,
};

use crate::{
    models::{
        rss::{AnimeRssRecord, RssItem},
        torrent::Torrent,
    },
    provider::db::db_provider::{AnimeProvider, RssProvider, RuleProvider},
};

#[derive(Debug, Serialize, Deserialize, Clone)]
struct AnimeTask {
    pub info: AnimeInfo,
    pub is_canncel: bool,
}

#[derive(Clone)]
pub struct Tasker {
    rss_db: RssProvider,
    rss_http_client: Arc<RssHttpClient>,
    anime_db: AnimeProvider,
    anime: Arc<AnimeTracker>,
    rule_db: RuleProvider,

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
        rss_http_client: Arc<RssHttpClient>,
        anime_db: AnimeProvider,
        anime: Arc<AnimeTracker>,
        rule_db: RuleProvider,
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
                    if s.rss_send_map.lock().unwrap().len() > 0 {
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

    async fn start_listener(&self, anime: &AnimeInfo) -> Result<(), Error> {
        // for i in anime.iter() {
        let mut rx = self.anime_broadcast.subscribe();
        // let anime = i.clone();
        let mut send_map = self.rss_send_map.lock().unwrap();
        // 如果已经启动过协程监听，则跳过
        if send_map.get(&anime.id).is_some() {
            return Ok(());
        }
        let (tx, mut recv) = mpsc::channel(20);
        let mut broadcast_recv = self.anime_rss_broadcast.subscribe();
        send_map.insert(anime.id, tx);
        let s = self.clone();
        let anime = anime.clone();
        spawn(async move {
            tracing::debug!("spawn anime: {:?}", &anime);
            loop {
                select! {
                    Ok(msg) = rx.recv() => {
                        if msg.is_canncel && anime.id == msg.info.id {
                            return;
                        }
                    }
                    Some(msg) = recv.recv() => {
                        s.check_anime_rules(msg, &anime).await;
                    }
                    Ok(msg) = broadcast_recv.recv() => {
                        s.check_anime_rules(msg,&anime).await;
                    }
                }
            }
        });
        // }
        Ok(())
    }

    async fn init_anime_listener(&self) -> Result<(), Error> {
        let anime = self
            .anime_db
            .get_calender()
            .expect("init_anime_listener get_calender failed")
            .ok_or(Error::msg("anime list is empty"))?;
        for i in anime.iter() {
            if i.status {
                if let Err(e) = self.start_listener(&i.anime_info).await {
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
            .expect("check_update get_all_rules failed")
            .ok_or(Error::msg("rss list is empty"))?;
        let anime_list = self
            .anime_db
            .get_calender()
            .expect("check_update get_calender failed")
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
                    };
                    if let Err(err) = self.anime_rss_broadcast.send(ri.clone()) {
                        tracing::error!("broadcast rss item to chan failed, {}", err);
                    }
                }
            }
            // 特定番剧的搜索RSS则只给该番发送
            if let Some(search_url) = item.search_url.clone() {
                anime_list.iter().for_each(|anime| {
                    anime
                        .anime_info
                        .names()
                        .iter()
                        .filter_map(|name| match formatx!(&search_url, &name) {
                            Ok(url) => Some(url),
                            Err(_) => None,
                        })
                        .for_each(|url| {
                            if let Some(chan) =
                                self.rss_send_map.lock().unwrap().get(&anime.anime_info.id)
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
                                            if let Err(err) = chan
                                                .send(RssItem {
                                                    title: item.title.clone().unwrap(),
                                                    magnet: url.to_string(),
                                                })
                                                .await
                                            {
                                                tracing::error!(
                                                    "send rss item to chan failed, {}",
                                                    err
                                                );
                                            }
                                        }
                                    }
                                });
                            }
                        })
                })
            }
        }
        Ok(())
    }

    async fn sync_calender(&self) -> Result<(), Error> {
        let anime = self
            .anime
            .get_calender()
            .await
            .expect("sync_calender get_calender failed");
        for i in anime.iter() {
            if let Err(e) = self.start_listener(i).await {
                tracing::error!("sync_calender start_listener failed, error: {}", e);
            }
        }
        Ok(self
            .anime_db
            .set_calenders(anime)
            .expect("sync_calender set failed"))
    }

    async fn check_anime_rules(&self, msg: RssItem, anime: &AnimeInfo) {
        // tracing::debug!("check_anime_rules rss: {:?}", msg);
        if let Ok(Some(rules)) = self.rule_db.get_all_rules() {
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
                            // TODO:
                            // 检查磁力链接是否是相同的，注意去掉tracker
                            // 发送磁力链接到qbit下载
                            // 记录下载的内容到数据库
                            tracing::debug!(
                                "anime: {}\nbt: {}\nrule: {}",
                                &anime.name,
                                &msg.title,
                                &i.re
                            );
                            if let Ok(info_hash) = Self::get_info_hash(&msg.magnet).await {
                                let r = self.anime_db.get_anime_recode(anime.id, &info_hash);
                                if r.is_err() {
                                    continue;
                                }
                                if r.unwrap().is_none() {
                                    if let Err(e) = self.anime_db.set_anime_recode(
                                        anime.id,
                                        AnimeRssRecord {
                                            title: msg.title,
                                            magnet: msg.magnet,
                                            rule_name: rule.name.clone(),
                                            info_hash,
                                        },
                                    ) {
                                        tracing::error!(
                                            "check_anime_rules set_calender failed, error: {}",
                                            e
                                        );
                                    }
                                }
                                // 检查是否已经完结
                                // if let Ok(anime_list) =
                                //     self.anime_db.get_anime_rss_recodes(anime.id)
                                // {
                                //     if anime_list.is_none() {
                                //         return;
                                //     }
                                //     let anime_list = anime_list.unwrap();
                                //     if anime_list.len() >= anime.eps as usize {}
                                // }
                            }
                            return;
                        }
                    }
                }
            }
        }
    }

    pub async fn get_info_hash(url: &str) -> Result<String, Error> {
        if let Some(hash_info) = Self::get_magnet_info_hash(url) {
            if hash_info.len() <= 32 {
                Ok(
                    base32::decode(Alphabet::Rfc4648 { padding: true }, &hash_info)
                        .ok_or(Error::msg("base32 to hex failed"))?
                        .iter()
                        .map(|byte| format!("{:02x}", byte))
                        .collect::<String>(),
                )
            } else {
                Ok(hash_info)
            }
        } else {
            let bytes = reqwest::get(url).await?.bytes().await?;
            let info: Torrent = serde_bencode::from_bytes(&bytes.to_vec())?;
            let mut hasher = Sha1::new();
            let info = serde_bencode::to_bytes(&info.info)?;
            hasher.update(info);
            let info_hash = format!("{:x}", hasher.finalize());
            return Ok(info_hash.to_lowercase());
        }
    }

    fn get_magnet_info_hash(magnet_link: &str) -> Option<String> {
        let url = Url::parse(magnet_link).ok()?;
        let xt_param = url.query_pairs().find(|(k, _)| k == "xt")?;
        let info_hash = xt_param.1.strip_prefix("urn:btih:")?;

        Some(info_hash.to_string())
    }
}
