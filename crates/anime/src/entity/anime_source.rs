use std::sync::Arc;

use common::shared::error::Error;

use crate::entity::{
    cap::{AnimeLookupProvider, AnimeSeasonalProvider},
    model::{AnimeMetadata, AnimeSearchResult, AnimeSourceTarget},
};

#[derive(Clone)]
pub struct AnimeSources {
    anime_provider: Arc<dyn AnimeLookupProvider>,
    seasonal_providers: Vec<Arc<dyn AnimeSeasonalProvider>>,
}

impl AnimeSources {
    pub fn new(
        searcher: Arc<dyn AnimeLookupProvider>,
        sources: Vec<Arc<dyn AnimeSeasonalProvider>>,
    ) -> Self {
        Self {
            anime_provider: searcher,
            seasonal_providers: sources,
        }
    }

    pub async fn search(&self, keyword: &str) -> Result<Vec<AnimeSearchResult>, Error> {
        self.anime_provider
            .search(keyword)
            .await
            .map_err(|e| Error::external("anime source search failed", e))
    }

    pub async fn lookup_by_id(&self, id: i64) -> Result<Option<AnimeMetadata>, Error> {
        self.anime_provider
            .lookup(id)
            .await
            .map_err(|e| Error::external("anime source lookup failed", e))
    }

    pub async fn sync(&self) -> Result<Vec<AnimeMetadata>, Error> {
        let mut metadata: Vec<AnimeMetadata> = vec![];
        for provider in &self.seasonal_providers {
            match provider.get().await {
                Ok(data) => {
                    for i in data {
                        if !metadata.iter().any(|existing| {
                            for y in &i.external_link {
                                let AnimeSourceTarget::Bangumi = y.target else {
                                    continue;
                                };
                                for x in &existing.external_link {
                                    let AnimeSourceTarget::Bangumi = x.target else {
                                        continue;
                                    };
                                    if y.id == x.id {
                                        return true;
                                    }
                                }
                            }

                            false
                        }) {
                            metadata.push(i);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "anime source sync provider {} failed, error = {e}",
                        provider.name()
                    );
                }
            }
        }
        Ok(metadata)
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use chrono::NaiveDate;

    use super::*;
    use crate::entity::model::{
        AnimeAirWeekday, AnimeEx, AnimeIdType, AnimeLangTarget, AnimeTitle,
    };

    const BANGUMI_ID: i64 = 558064;

    /// 可脚本化的季度数据源，只实现生产 trait。
    struct FakeSeasonalProvider {
        name: &'static str,
        fail: bool,
        data: Vec<AnimeMetadata>,
    }

    #[async_trait]
    impl AnimeSeasonalProvider for FakeSeasonalProvider {
        async fn get(&self) -> anyhow::Result<Vec<AnimeMetadata>> {
            if self.fail {
                anyhow::bail!("seasonal provider {} unavailable", self.name);
            }
            Ok(self.data.clone())
        }

        fn name(&self) -> &str {
            self.name
        }
    }

    /// 可脚本化的检索数据源，只实现生产 trait。
    struct FakeLookupProvider {
        fail: bool,
        result: Option<AnimeMetadata>,
    }

    #[async_trait]
    impl AnimeLookupProvider for FakeLookupProvider {
        async fn search(&self, keyword: &str) -> anyhow::Result<Vec<AnimeSearchResult>> {
            if self.fail {
                anyhow::bail!("lookup source unavailable for keyword {keyword}");
            }
            Ok(vec![AnimeSearchResult {
                name: keyword.to_string(),
                name_cn: Some("女主角？圣女？不，我是杂役女仆（自豪）！".to_string()),
                id: BANGUMI_ID,
            }])
        }

        async fn lookup(&self, id: i64) -> anyhow::Result<Option<AnimeMetadata>> {
            if self.fail {
                anyhow::bail!("lookup source unavailable for id {id}");
            }
            if id == BANGUMI_ID {
                Ok(self.result.clone())
            } else {
                Ok(None)
            }
        }
    }

    fn metadata(bangumi_id: i64, title: &str) -> AnimeMetadata {
        AnimeMetadata {
            series_metadata: None,
            external_link: vec![AnimeEx {
                id: AnimeIdType::Int(bangumi_id),
                target: AnimeSourceTarget::Bangumi,
                r#type: Some("tv".to_string()),
            }],
            titles: vec![AnimeTitle {
                name: title.to_string(),
                match_name: title.to_string(),
                target: AnimeLangTarget::JP,
                origin: true,
            }],
            air_weekday: AnimeAirWeekday::Wednesday,
            air_date: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
            air_quarter: 202607,
            season: vec![],
        }
    }

    fn bangumi_id(item: &AnimeMetadata) -> i64 {
        item.external_link
            .iter()
            .find_map(|link| match (&link.target, &link.id) {
                (AnimeSourceTarget::Bangumi, AnimeIdType::Int(id)) => Some(*id),
                _ => None,
            })
            .unwrap()
    }

    fn seasonal(fail: bool, data: Vec<AnimeMetadata>) -> Arc<dyn AnimeSeasonalProvider> {
        Arc::new(FakeSeasonalProvider {
            name: "fake-seasonal",
            fail,
            data,
        })
    }

    fn lookup(fail: bool, result: Option<AnimeMetadata>) -> Arc<dyn AnimeLookupProvider> {
        Arc::new(FakeLookupProvider { fail, result })
    }

    fn sources(
        lookup: Arc<dyn AnimeLookupProvider>,
        seasonal: Vec<Arc<dyn AnimeSeasonalProvider>>,
    ) -> AnimeSources {
        AnimeSources::new(lookup, seasonal)
    }

    #[tokio::test]
    async fn sync_dedups_bangumi_ids_and_keeps_provider_order() {
        let sources = sources(
            lookup(false, None),
            vec![
                seasonal(false, vec![metadata(1, "第一"), metadata(2, "第二")]),
                // 第 2 条与第一个数据源的 id=2 重复，应被跳过
                seasonal(false, vec![metadata(2, "第二重复"), metadata(3, "第三")]),
            ],
        );

        let synced = sources.sync().await.unwrap();
        let ids: Vec<i64> = synced.iter().map(bangumi_id).collect();
        assert_eq!(ids, vec![1, 2, 3]);

        let titles: Vec<&str> = synced.iter().map(|i| i.titles[0].name.as_str()).collect();
        assert_eq!(titles, vec!["第一", "第二", "第三"]);
    }

    #[tokio::test]
    async fn sync_skips_failed_provider_and_continues() {
        let sources = sources(
            lookup(false, None),
            vec![
                seasonal(true, vec![metadata(9, "不该出现")]),
                seasonal(false, vec![metadata(7, "第七")]),
            ],
        );

        // 单个数据源失败只记录日志，整体仍然成功且后续数据源照常合并
        let synced = sources.sync().await.unwrap();
        assert_eq!(synced.len(), 1);
        assert_eq!(synced[0].titles[0].name, "第七");
        assert_eq!(bangumi_id(&synced[0]), 7);
    }

    #[tokio::test]
    async fn sync_without_provider_returns_empty() {
        let sources = sources(lookup(false, None), vec![]);
        assert!(sources.sync().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn search_returns_provider_results() {
        let sources = sources(lookup(false, None), vec![]);
        let found = sources.search("女主角").await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "女主角");
        assert_eq!(found[0].id, BANGUMI_ID);
        assert!(found[0].name_cn.is_some());
    }

    #[tokio::test]
    async fn search_error_is_mapped_to_external_contract_mismatch() {
        let sources = sources(lookup(true, None), vec![]);
        let err = sources.search("女主角").await.expect_err("should fail");
        assert!(matches!(
            err,
            Error::ExternalContractMismatch { ref context, .. }
                if context == "anime source search failed"
        ));
    }

    #[tokio::test]
    async fn lookup_by_id_returns_provider_result() {
        let sources = sources(lookup(false, Some(metadata(BANGUMI_ID, "女主角"))), vec![]);
        let found = sources.lookup_by_id(BANGUMI_ID).await.unwrap();
        assert_eq!(
            found.as_ref().map(|i| i.titles[0].name.as_str()),
            Some("女主角")
        );

        let missing = sources.lookup_by_id(1).await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn lookup_error_is_mapped_to_external_contract_mismatch() {
        let sources = sources(lookup(true, None), vec![]);
        let err = sources
            .lookup_by_id(BANGUMI_ID)
            .await
            .expect_err("should fail");
        assert!(matches!(
            err,
            Error::ExternalContractMismatch { ref context, .. }
                if context == "anime source lookup failed"
        ));
    }
}
