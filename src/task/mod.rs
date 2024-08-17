pub mod task;

#[cfg(test)]
mod tests {

    use task::Tasker;

    use super::*;

    #[tokio::test]
    async fn test_get_info_hash() {
        let info_hash = Tasker::get_info_hash("https://nyaa.si/download/1860859.torrent")
            .await
            .unwrap();
        assert_eq!(
            info_hash,
            "416cff2217b776196cc67b76b032734094377675".to_string()
        )
    }
}
