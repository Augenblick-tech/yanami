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
use formatx::formatx;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::{
    select, spawn,
    sync::{
        broadcast::{self},
        mpsc,
    },
    time,
};

use crate::{
    models::rss::RssItem,
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

    hander_anime_broadcast: Arc<broadcast::Receiver<AnimeTask>>,
    hander_anime_rss_broadcast: Arc<broadcast::Receiver<RssItem>>,
}

impl Tasker {
    pub fn new(
        rss: RssProvider,
        rss_http_client: Arc<RssHttpClient>,
        anime_db: AnimeProvider,
        anime: Arc<AnimeTracker>,
        rule_db: RuleProvider,
    ) -> Self {
        let (ab, abr) = broadcast::channel::<AnimeTask>(10);
        let (arb, arbr) = broadcast::channel::<RssItem>(10);
        Tasker {
            rss_db: rss,
            rss_http_client,
            anime_db,
            anime,
            rule_db,
            anime_broadcast: ab,
            anime_rss_broadcast: arb,
            rss_send_map: Arc::new(Mutex::new(HashMap::new())),
            hander_anime_broadcast: Arc::new(abr),
            hander_anime_rss_broadcast: Arc::new(arbr),
        }
    }
    pub async fn run(&self) {
        let mut sync_calender_ticker = time::interval(Duration::from_secs(12 * 60 * 60));
        let mut check_update_ticker = time::interval(Duration::from_secs(5 * 60));
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
            let r = self.rss_http_client.get_channel(&item.url).await;
            if r.is_err() {
                tracing::error!(
                    "check_update get_calender {} failed, {}",
                    &item.url,
                    r.unwrap_err()
                );
                continue;
            }
            let rsp = r.unwrap();
            // 全站RSS则获取后给所有番剧发送广播
            for i in rsp.items.iter() {
                tracing::debug!("check_update rss: {:?}", i);
                if i.title.is_none() || i.enclosure.is_none() {
                    continue;
                }
                let ri = RssItem {
                    title: i.title.clone().unwrap(),
                    magnet: i.enclosure.clone().unwrap().url,
                };
                if let Err(err) = self.anime_rss_broadcast.send(ri) {
                    tracing::error!("broadcast rss item to chan failed, {}", err);
                }
            }
            // 特定番剧的搜索RSS则只给该番发送
            if let Some(search_url) = item.search_url.clone() {
                anime_list.iter().for_each(|anime| {
                    let urls = [
                        formatx!(&search_url, &anime.name_tw),
                        formatx!(&search_url, &anime.name_cn),
                        formatx!(&search_url, &anime.name),
                    ];
                    urls.into_iter().filter(|url| url.is_ok()).for_each(|url| {
                        let url = url.unwrap();
                        if let Some(chan) = self.rss_send_map.lock().unwrap().get(&anime.id) {
                            let chan = chan.clone();
                            let rss_http_client = self.rss_http_client.clone();
                            spawn(async move {
                                if let Ok(rsp) = rss_http_client.get_channel(&url).await {
                                    for item in rsp.items.iter() {
                                        if let Err(err) = chan
                                            .send(RssItem {
                                                title: item.title.clone().unwrap(),
                                                magnet: item.enclosure.clone().unwrap().url,
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
            let mut rx = self.anime_broadcast.subscribe();
            let anime = i.clone();
            let mut send_map = self.rss_send_map.lock().unwrap();
            // 如果已经启动过协程监听，则跳过
            if send_map.get(&i.id).is_some() {
                continue;
            }
            let (tx, mut recv) = mpsc::channel(20);
            let mut broadcast_recv = self.anime_rss_broadcast.subscribe();
            send_map.insert(i.id, tx);
            let s = self.clone();
            spawn(async move {
                tracing::debug!("sync_calender spawn anime: {:?}", &anime);
                loop {
                    select! {
                        Ok(msg) = rx.recv() => {
                            if msg.is_canncel && anime.id == msg.info.id {
                                return;
                            }
                        }
                        Some(msg) = recv.recv() => {
                            s.check_anime_rules(msg, &anime);
                        }
                        Ok(msg) = broadcast_recv.recv() => {
                            s.check_anime_rules(msg,&anime);
                        }
                    }
                }
            });
        }
        Ok(self
            .anime_db
            .set_calender(anime)
            .expect("sync_calender set failed"))
    }

    fn check_anime_rules(&self, msg: RssItem, anime: &AnimeInfo) {
        if let Ok(Some(rules)) = self.rule_db.get_all_rules() {
            for rule in rules.iter() {
                for i in rule.rules.iter() {
                    if Regex::new(&i.re).unwrap().is_match(&msg.title) {
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
                        return;
                    }
                }
            }
        }
    }
}
