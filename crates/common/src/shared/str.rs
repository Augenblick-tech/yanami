use simd_normalizer::UnicodeNormalization;

// nfkc_to_lowercase 将字符串nfkc化并去除空格转小写
pub fn nfkc_to_lowercase(str: &str) -> String {
    str.nfkc()
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

// to_search_keywords 将字符串按空格切割并舍弃特殊字符
pub fn to_search_keywords(str: &str) -> Vec<String> {
    str.chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

pub fn to_safe_filename(str: &str) -> String {
    str.replace(&['/', '\\', ':', '*', '?', '"', '<', '>', '|'][..], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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

    #[test]
    fn test_to_safe_filename_replaces_illegal_characters() {
        // 非法字符会被替换成空格，然后按空白折叠
        assert_eq!(
            to_safe_filename("[LoliHouse] 番名/第01话: <测试>?\"*|"),
            "[LoliHouse] 番名 第01话 测试"
        );

        // 反斜杠同样属于非法字符
        assert_eq!(to_safe_filename("a\\b"), "a b");

        // 折叠连续空白与制表符
        assert_eq!(to_safe_filename("a   b\tc"), "a b c");
    }

    #[test]
    fn test_to_safe_filename_empty_input() {
        assert_eq!(to_safe_filename(""), "");
        assert_eq!(to_safe_filename("   "), "");
        // 只有非法字符时，全部被替换为空格后再折叠为空串
        assert_eq!(to_safe_filename("///"), "");
        assert_eq!(to_safe_filename(":*?\"<>|"), "");
    }

    #[test]
    fn test_to_safe_filename_keeps_chinese_text() {
        assert_eq!(
            to_safe_filename("葬送的芙莉莲 第01话"),
            "葬送的芙莉莲 第01话"
        );
        assert_eq!(
            to_safe_filename("【合集】进击的巨人: 最终季"),
            "【合集】进击的巨人 最终季"
        );
    }

    #[test]
    fn test_nfkc_to_lowercase_full_width_normalisation() {
        // 全角字母与数字被 NFKC 归一化为半角，并统一转小写
        assert_eq!(nfkc_to_lowercase("ＡＢＣ１２３"), "abc123");
        assert_eq!(nfkc_to_lowercase("Ｈｅｌｌｏ"), "hello");

        // 全角空格属于空白，归一化后会被过滤掉
        assert_eq!(nfkc_to_lowercase("Ｈｅｌｌｏ　Ｗｏｒｌｄ"), "helloworld");

        // 全角标点在 NFKC 下同样被归一化
        assert_eq!(nfkc_to_lowercase("Kurumi-chan！！"), "kurumi-chan!!");
        assert_eq!(nfkc_to_lowercase("  全角　空格  "), "全角空格");
    }
}
