use base32::Alphabet;

use crate::entity::model::ResourceBaseData;

#[derive(Debug, Clone)]
pub struct ResourceEntity {
    data: ResourceBaseData,
}
const PREFIX: &str = "magnet:?xt=urn:btih:";

impl ResourceEntity {
    pub(super) fn new(data: ResourceBaseData) -> Self {
        Self { data }
    }

    pub fn id(&self) -> &[u8; 20] {
        &self.data.info_hash
    }

    pub fn title(&self) -> &str {
        &self.data.title
    }

    pub fn match_title(&self) -> &str {
        &self.data.match_title
    }

    pub fn url(&self) -> &str {
        &self.data.url
    }

    pub fn published_at(&self) -> i64 {
        self.data.published_at
    }

    /// 根据 Base32 编码的 info_hash 生成磁力链接。
    ///
    /// 生成的链接格式为 `magnet:?xt=urn:btih:<info_hash>`。
    pub fn magnet_base32(&self) -> String {
        let mut s = String::with_capacity(PREFIX.len() + 40);
        s.push_str(PREFIX);
        s.push_str(&base32::encode(
            Alphabet::Rfc4648 { padding: false },
            &self.data.info_hash,
        ));
        s
    }

    /// 根据 Hex 编码的 info_hash 生成磁力链接。
    ///
    /// 生成的链接格式为 `magnet:?xt=urn:btih:<info_hash>`。
    pub fn magnet_hex(&self) -> String {
        let mut s = String::with_capacity(PREFIX.len() + 40);
        s.push_str(PREFIX);
        s.push_str(&hex::encode(self.data.info_hash));
        s
    }
}
