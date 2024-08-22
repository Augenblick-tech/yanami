pub mod task;

#[cfg(test)]
mod tests {

    use task::Tasker;

    use crate::models::rss::AnimeRssRecord;

    use super::*;

    #[tokio::test]
    async fn test_get_info_hash() {
        let info_hash = Tasker::get_info_hash(
            "https://dl.dmhy.org/2024/07/10/80fe910222908637baea6adeb44de49f0c512474.torrent",
        )
        .await
        .unwrap();
        assert_eq!(
            info_hash,
            "80fe910222908637baea6adeb44de49f0c512474".to_string()
        );
        let info_hash =
            Tasker::get_info_hash("magnet:?xt=urn:btih:QD7JCARCSCDDPOXKNLPLITPET4GFCJDU")
                .await
                .unwrap();
        assert_eq!(
            info_hash,
            "80fe910222908637baea6adeb44de49f0c512474".to_string()
        );
    }

    #[test]
    fn test_get_season_eps() {
        let list = vec![
            AnimeRssRecord {
                title: "[ANi] Make Heroine ga Oosugiru / 敗北女角太多了！ - 04 [1080P][Baha][WEB-DL][AAC AVC][CHT][MP4]".to_string(),
                magnet: "".to_string(),
                rule_name: "".to_string(),
                info_hash: "".to_string(),
            },
            AnimeRssRecord {
                title: "[ANi] Make Heroine ga Oosugiru / 敗北女角太多了！ - 03 [1080P][Baha][WEB-DL][AAC AVC][CHT][MP4]".to_string(),
                magnet: "".to_string(),
                rule_name: "".to_string(),
                info_hash: "".to_string(),
            },
            AnimeRssRecord {
                title: "[ANi] Make Heroine ga Oosugiru / 敗北女角太多了！ - 04 [1080P][Baha][WEB-DL][AAC AVC][CHT][MP4]v2".to_string(),
                magnet: "".to_string(),
                rule_name: "".to_string(),
                info_hash: "".to_string(),
            },
            AnimeRssRecord {
                title: "[ANi] Make Heroine ga Oosugiru / 敗北女角太多了！ - 02 [1080P][Baha][WEB-DL][AAC AVC][CHT][MP4]".to_string(),
                magnet: "".to_string(),
                rule_name: "".to_string(),
                info_hash: "".to_string(),
            },
        ];
        let eps = Tasker::get_season_eps(list).unwrap();
        assert_eq!(eps, vec![2, 3, 4]);
    }
}
