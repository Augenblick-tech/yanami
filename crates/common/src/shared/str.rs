use simd_normalizer::UnicodeNormalization;

// nfkc_to_lowercase 将字符串nfkc化并去除空格转小写
pub fn nfkc_to_lowercase(str: &str) -> String {
    str.nfkc()
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

pub fn to_search_keywords(str: &str) -> Vec<String> {
    str.chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_search_keywords() {
        assert_eq!(
            to_search_keywords("クレバテスⅡ-魔獣の王と偽りの勇者伝承-"),
            vec!["クレバテスⅡ", "魔獣の王と偽りの勇者伝承"]
        );

        assert_eq!(
            to_search_keywords("Clevatess II-魔兽之王与虚假的勇者传承-"),
            vec!["Clevatess", "II", "魔兽之王与虚假的勇者传承"]
        );

        assert_eq!(
            to_search_keywords("Clevatess.Majuu.no.Ou.to.Akago.to.Kabane.no.Yuusha"),
            vec![
                "Clevatess",
                "Majuu",
                "no",
                "Ou",
                "to",
                "Akago",
                "to",
                "Kabane",
                "no",
                "Yuusha"
            ]
        );

        assert_eq!(
            to_search_keywords("Clevatess -魔獸之王與嬰兒與屍之勇者-"),
            vec!["Clevatess", "魔獸之王與嬰兒與屍之勇者"]
        );

        assert_eq!(
            to_search_keywords(
                "Clevatess: The King of Devil Beasts, The Baby and the Brave of Undead"
            ),
            vec![
                "Clevatess",
                "The",
                "King",
                "of",
                "Devil",
                "Beasts",
                "The",
                "Baby",
                "and",
                "the",
                "Brave",
                "of",
                "Undead"
            ]
        );
    }
}
