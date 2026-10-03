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

#[cfg(test)]
mod tests {
    use super::*;

    // 真实 info_hash：取自 https://archive.org/download/100200300_359/100200300_359_archive.torrent
    // 该 torrent 的 info 段 SHA-1 值
    const INFO_HASH: [u8; 20] = [
        0xcf, 0x8c, 0xd6, 0xac, 0x7f, 0x30, 0xed, 0x26, 0x2d, 0xdc, 0x42, 0x00, 0x9a, 0xec, 0x0d,
        0x53, 0x0e, 0x6c, 0xf7, 0x1f,
    ];

    // 上面真实 info_hash 的 Hex 编码
    const INFO_HASH_HEX: &str = "cf8cd6ac7f30ed262ddc42009aec0d530e6cf71f";

    // 上面真实 info_hash 的 Base32 编码（RFC4648，无 padding）
    const INFO_HASH_BASE32: &str = "Z6GNNLD7GDWSMLO4IIAJV3ANKMHGZ5Y7";

    fn entity() -> ResourceEntity {
        ResourceEntity::new(ResourceBaseData {
            title: "黑化吧！圣女大人 第二季 第01话".to_string(),
            match_title: "黑化吧 圣女大人".to_string(),
            url: "magnet:?xt=urn:btih:cf8cd6ac7f30ed262ddc42009aec0d530e6cf71f".to_string(),
            info_hash: INFO_HASH,
            published_at: 1_790_959_920,
        })
    }

    #[test]
    fn test_magnet_prefix_constant() {
        assert_eq!(PREFIX, "magnet:?xt=urn:btih:");
    }

    #[test]
    fn test_magnet_hex_round_trip() {
        let magnet = entity().magnet_hex();
        assert_eq!(magnet, format!("magnet:?xt=urn:btih:{INFO_HASH_HEX}"));

        let suffix = magnet
            .strip_prefix(PREFIX)
            .expect("magnet_hex must start with PREFIX");
        assert_eq!(suffix, INFO_HASH_HEX);
        // 解码后必须还原出原始 info_hash
        assert_eq!(
            hex::decode(suffix).expect("hex should decode"),
            INFO_HASH.to_vec()
        );
    }

    #[test]
    fn test_magnet_base32_round_trip() {
        let magnet = entity().magnet_base32();
        assert_eq!(magnet, format!("magnet:?xt=urn:btih:{INFO_HASH_BASE32}"));

        let suffix = magnet
            .strip_prefix(PREFIX)
            .expect("magnet_base32 must start with PREFIX");
        // 20 字节按 RFC4648 Base32 编码后固定为 32 个字符（无需 padding）
        assert_eq!(suffix.len(), 32);
        assert_eq!(
            base32::decode(Alphabet::Rfc4648 { padding: false }, suffix)
                .expect("base32 should decode"),
            INFO_HASH.to_vec()
        );
    }

    #[test]
    fn test_getters_expose_base_data() {
        let entity = entity();
        assert_eq!(entity.id(), &INFO_HASH);
        assert_eq!(entity.title(), "黑化吧！圣女大人 第二季 第01话");
        assert_eq!(entity.match_title(), "黑化吧 圣女大人");
        assert_eq!(
            entity.url(),
            "magnet:?xt=urn:btih:cf8cd6ac7f30ed262ddc42009aec0d530e6cf71f"
        );
        assert_eq!(entity.published_at(), 1_790_959_920);
    }
}
