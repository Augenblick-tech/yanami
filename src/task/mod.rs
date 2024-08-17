pub mod task;

#[cfg(test)]
mod tests {

    use task::Tasker;

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
}
