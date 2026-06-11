use simd_normalizer::UnicodeNormalization;

// nfkc_to_lowercase 将字符串nfkc化并去除空格转小写
pub fn nfkc_to_lowercase(str: &str) -> String {
    str.nfkc()
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect()
}
