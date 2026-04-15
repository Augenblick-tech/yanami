use std::collections::{HashMap, HashSet};
use std::time::Duration;

use anime::gateway::{YucAnimeEntry, YucLabeledValue, YucLink, YucSeasonCalendarPage};
use anyhow::{anyhow, Error};
use chrono::{Datelike, Local, NaiveDate};
use domain::shared::error::DomainError;
use reqwest::{Client, Url};
use scraper::{ElementRef, Html, Selector};

#[derive(Debug)]
pub struct YucClient {
    client: Client,
    base_url: String,
}

impl YucClient {
    pub fn new() -> Result<Self, Error> {
        Self::with_base_url("https://yuc.wiki")
    }

    pub fn with_base_url(base_url: &str) -> Result<Self, Error> {
        Ok(Self {
            client: Client::builder()
                .user_agent("yanami yuc infrastructure")
                .timeout(Duration::from_secs(30))
                .build()?,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    pub fn season_page_url(year: i32, month: u32) -> Result<String, Error> {
        Self::season_page_url_with_base("https://yuc.wiki", year, month)
    }

    pub fn season_page_url_with_base(
        base_url: &str,
        year: i32,
        month: u32,
    ) -> Result<String, Error> {
        if !matches!(month, 1 | 4 | 7 | 10) {
            return Err(anyhow!("season month must be one of 1, 4, 7, 10"));
        }
        Ok(format!(
            "{}/{year}{month:02}/",
            base_url.trim_end_matches('/')
        ))
    }

    pub fn season_of_date(date: NaiveDate) -> (i32, u32) {
        match date.month() {
            1..=3 => (date.year(), 1),
            4..=6 => (date.year(), 4),
            7..=9 => (date.year(), 7),
            10..=12 => (date.year(), 10),
            _ => unreachable!(),
        }
    }

    pub fn current_season_page_url() -> Result<String, Error> {
        let today = Local::now().date_naive();
        let (year, month) = Self::season_of_date(today);
        Self::season_page_url(year, month)
    }

    pub async fn fetch_current_season_calendar(
        &self,
    ) -> Result<YucSeasonCalendarPage, DomainError> {
        let today = Local::now().date_naive();
        let (year, month) = Self::season_of_date(today);
        self.fetch_season_calendar(year, month).await
    }

    pub async fn fetch_season_calendar(
        &self,
        year: i32,
        month: u32,
    ) -> Result<YucSeasonCalendarPage, DomainError> {
        let page_url = Self::season_page_url_with_base(&self.base_url, year, month)
            .map_err(|error| DomainError::external("yuc season url build failed", error))?;
        self.fetch_season_calendar_by_url(&page_url).await
    }

    pub async fn fetch_season_calendar_by_url(
        &self,
        page_url: &str,
    ) -> Result<YucSeasonCalendarPage, DomainError> {
        let url = Url::parse(page_url)
            .map_err(|error| DomainError::external("yuc page url parse failed", error))?;
        let html = self
            .client
            .get(url.clone())
            .send()
            .await
            .map_err(|error| DomainError::external("yuc page request failed", error))?
            .text()
            .await
            .map_err(|error| DomainError::external("yuc page body read failed", error))?;
        parse_yuc_page(&url, &html)
            .map_err(|error| DomainError::external("yuc page parse failed", error))
    }
}

#[derive(Debug, Clone)]
struct SchedulePreviewEntry {
    weekday: Option<String>,
    time: Option<String>,
    start_date: Option<String>,
    note: Option<String>,
    image_url: Option<String>,
    streaming_links: Vec<YucLink>,
}

fn parse_yuc_page(base_url: &Url, html: &str) -> Result<YucSeasonCalendarPage, Error> {
    let document = Html::parse_document(html);
    let root = select_content_root(&document).ok_or_else(|| anyhow!("yuc content root missing"))?;
    let page_title =
        extract_page_title(&document).ok_or_else(|| anyhow!("yuc page title missing"))?;
    let updated_at = extract_updated_at(&document);
    let season_code = extract_season_code(base_url)?;
    let preview_map = parse_schedule_previews(base_url, &root)?;
    let entries = parse_detail_entries(base_url, &root, &preview_map)?;

    Ok(YucSeasonCalendarPage {
        page_title,
        page_url: base_url.as_str().to_string(),
        season_code,
        updated_at,
        entries,
    })
}

fn select_content_root<'a>(document: &'a Html) -> Option<ElementRef<'a>> {
    for raw in [
        ".post-body",
        "article.post-block",
        "body",
        "main",
        "article",
    ] {
        let selector = Selector::parse(raw).ok()?;
        if let Some(root) = document.select(&selector).next() {
            return Some(root);
        }
    }
    None
}

fn extract_page_title(document: &Html) -> Option<String> {
    let selector = Selector::parse("h1.post-title, title").ok()?;
    document
        .select(&selector)
        .next()
        .map(|title| clean_text(&title.text().collect::<Vec<_>>().join(" ")))
        .filter(|title| !title.is_empty())
}

fn extract_updated_at(document: &Html) -> Option<String> {
    let selector = Selector::parse("time[itemprop=\"dateModified\"]").ok()?;
    document
        .select(&selector)
        .next()
        .map(|time| clean_text(&time.text().collect::<Vec<_>>().join(" ")))
        .filter(|text| !text.is_empty())
}

fn extract_season_code(url: &Url) -> Result<String, Error> {
    let segment = url
        .path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .ok_or_else(|| anyhow!("yuc season code missing in url"))?;
    if segment.len() == 6 && segment.chars().all(|ch| ch.is_ascii_digit()) {
        Ok(segment.to_string())
    } else {
        Err(anyhow!("yuc season code invalid in url"))
    }
}

fn parse_schedule_previews(
    base_url: &Url,
    root: &ElementRef<'_>,
) -> Result<HashMap<String, SchedulePreviewEntry>, Error> {
    let float_block_selector = selector("div[style=\"float:left\"]")?;
    let date_block_selector = selector(".div_date")?;
    let title_selector = selector("td[class^=\"date_title_\"]")?;
    let time_selector = selector("p.imgtext4, p.imgtext5")?;
    let date_selector = selector("p.imgep, p.imgep2")?;
    let link_selector = selector("tr.tr_area a")?;
    let image_selector = selector(".div_date img")?;

    let mut previews = HashMap::new();

    for block in root.select(&float_block_selector) {
        if block.select(&date_block_selector).next().is_none() {
            continue;
        }

        let Some(title_zh) = first_title_text(&block, &title_selector) else {
            continue;
        };

        let time = first_text(&block, &time_selector);
        let mut start_date = None;
        let mut note = None;
        for item in block.select(&date_selector) {
            let text = clean_text(&item.text().collect::<Vec<_>>().join(" "));
            if text.is_empty() {
                continue;
            }
            if looks_like_schedule_date(&text) {
                start_date = Some(text);
            } else {
                note = Some(text);
            }
        }

        let mut streaming_links = Vec::new();
        for anchor in block.select(&link_selector) {
            if let Some(link) = parse_link(base_url, &anchor)? {
                streaming_links.push(link);
            }
        }
        dedup_links(&mut streaming_links);

        previews.insert(
            normalize_title(&title_zh),
            SchedulePreviewEntry {
                weekday: None,
                time,
                start_date,
                note,
                image_url: block
                    .select(&image_selector)
                    .next()
                    .map(|image| image_url(base_url, &image))
                    .transpose()?
                    .flatten(),
                streaming_links,
            },
        );
    }

    Ok(previews)
}

fn parse_detail_entries(
    base_url: &Url,
    root: &ElementRef<'_>,
    preview_map: &HashMap<String, SchedulePreviewEntry>,
) -> Result<Vec<YucAnimeEntry>, Error> {
    let table_selector = selector("table[width=\"500px\"]")?;
    let image_selector = selector("div[style=\"float:left\"] > img[width=\"180px\"]")?;
    let title_zh_selector = selector("p[class^=\"title_cn_r\"]")?;
    let title_original_selector = selector("p[class^=\"title_jp_r\"]")?;
    let source_type_selector = selector(".type_a_r, .type_b_r, .type_c_r, .type_e_r")?;
    let genre_selector = selector(".type_tag_r")?;
    let staff_selector = selector(".staff_r, .staff_r1, .staff_r2")?;
    let cast_selector = selector(".cast_r")?;
    let resource_link_selector = selector(".link_a_r a, .link_b_r a")?;
    let broadcast_selector = selector(".broadcast_r")?;
    let broadcast_extra_selector = selector(".broadcast_ex_r")?;

    let detail_images = root
        .select(&image_selector)
        .filter_map(|image| image_url(base_url, &image).transpose())
        .collect::<Result<Vec<_>, _>>()?;

    let mut entries = Vec::new();
    for (index, table) in root.select(&table_selector).enumerate() {
        let Some(title_zh) = first_title_text(&table, &title_zh_selector) else {
            continue;
        };
        let preview = preview_map.get(&normalize_title(&title_zh));

        let mut staff = Vec::new();
        let mut original_work = None;
        let mut original_work_extra = None;
        if let Some(staff_cell) = table.select(&staff_selector).next() {
            for line in extract_text_lines(&staff_cell) {
                if let Some((label, value)) = split_labeled_line(&line) {
                    if label == "原作" || label == "原案" {
                        original_work = Some(value.clone());
                    }
                    staff.push(YucLabeledValue { label, value });
                } else if line.starts_with('(') && line.ends_with(')') {
                    original_work_extra = Some(line);
                } else if let Some(last) = staff.last_mut() {
                    if !last.value.is_empty() {
                        last.value.push(' ');
                    }
                    last.value.push_str(&line);
                }
            }
        }

        let cast = table
            .select(&cast_selector)
            .next()
            .map(|cell| extract_text_lines(&cell))
            .unwrap_or_default();

        let mut resource_links = Vec::new();
        for anchor in table.select(&resource_link_selector) {
            if let Some(link) = parse_link(base_url, &anchor)? {
                resource_links.push(link);
            }
        }
        dedup_links(&mut resource_links);

        let mut broadcast_text = first_text(&table, &broadcast_selector);
        if let Some(extra) =
            first_text(&table, &broadcast_extra_selector).filter(|text| !text.is_empty())
        {
            match &mut broadcast_text {
                Some(text) if !text.is_empty() => {
                    text.push(' ');
                    text.push_str(&extra);
                }
                None => broadcast_text = Some(extra),
                _ => {}
            }
        }

        let entry = YucAnimeEntry {
            title_zh,
            title_original: first_title_text(&table, &title_original_selector),
            image_url: detail_images
                .get(index)
                .cloned()
                .or_else(|| preview.and_then(|item| item.image_url.clone())),
            source_type: first_text(&table, &source_type_selector),
            genre_text: first_text(&table, &genre_selector),
            original_work,
            original_work_extra,
            schedule_weekday: preview.and_then(|item| item.weekday.clone()),
            schedule_time: preview.and_then(|item| item.time.clone()),
            schedule_start_date: preview.and_then(|item| item.start_date.clone()),
            schedule_note: preview.and_then(|item| item.note.clone()),
            broadcast_text,
            streaming_links: preview
                .map(|item| item.streaming_links.clone())
                .unwrap_or_default(),
            resource_links,
            staff,
            cast,
        };

        entries.push(entry);
    }

    Ok(entries)
}

fn split_labeled_line(line: &str) -> Option<(String, String)> {
    let (label, value) = line.split_once('：').or_else(|| line.split_once(':'))?;
    Some((label.trim().to_string(), value.trim().to_string()))
}

fn normalize_title(title: &str) -> String {
    title.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn clean_text(text: &str) -> String {
    text.replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn selector(raw: &str) -> Result<Selector, Error> {
    Selector::parse(raw).map_err(|error| anyhow!("invalid yuc selector `{raw}`: {error}"))
}

fn resolve_href(base_url: &Url, href: &str) -> Result<String, Error> {
    base_url
        .join(href)
        .map(|url| url.to_string())
        .map_err(|error| anyhow!("yuc link resolve failed for {href}: {error}"))
}

fn first_text(root: &ElementRef<'_>, selector: &Selector) -> Option<String> {
    root.select(selector)
        .next()
        .map(|item| clean_text(&item.text().collect::<Vec<_>>().join(" ")))
        .filter(|text| !text.is_empty())
}

fn first_title_text(root: &ElementRef<'_>, selector: &Selector) -> Option<String> {
    root.select(selector)
        .next()
        .map(|item| {
            item.text()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .collect::<String>()
        })
        .filter(|text| !text.is_empty())
}

fn image_url(base_url: &Url, image: &ElementRef<'_>) -> Result<Option<String>, Error> {
    let href = image
        .value()
        .attr("data-src")
        .or_else(|| image.value().attr("src"))
        .or_else(|| image.value().attr("data-lazy-src"));
    match href {
        Some(href) => Ok(Some(resolve_href(base_url, href)?)),
        None => Ok(None),
    }
}

fn parse_link(base_url: &Url, anchor: &ElementRef<'_>) -> Result<Option<YucLink>, Error> {
    let Some(href) = anchor.value().attr("href") else {
        return Ok(None);
    };
    let label = clean_text(&anchor.text().collect::<Vec<_>>().join(" "));
    if label.is_empty() {
        return Ok(None);
    }
    Ok(Some(YucLink {
        label,
        href: resolve_href(base_url, href)?,
    }))
}

fn dedup_links(links: &mut Vec<YucLink>) {
    let mut seen = HashSet::new();
    links.retain(|link| seen.insert((link.label.clone(), link.href.clone())));
}

fn extract_text_lines(root: &ElementRef<'_>) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for child in root.children() {
        if let Some(text) = child.value().as_text() {
            let text = clean_text(text);
            if text.is_empty() {
                continue;
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(&text);
            continue;
        }

        let Some(element) = ElementRef::wrap(child) else {
            continue;
        };
        if element.value().name() == "br" {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            continue;
        }

        let text = clean_text(&element.text().collect::<Vec<_>>().join(" "));
        if text.is_empty() {
            continue;
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(&text);
    }

    if !current.is_empty() {
        lines.push(current);
    }

    lines
}

fn looks_like_schedule_date(text: &str) -> bool {
    let value = text.strip_suffix('~').unwrap_or(text);
    let Some((month, day)) = value.split_once('/') else {
        return false;
    };
    let Ok(month) = month.parse::<u32>() else {
        return false;
    };
    let Ok(day) = day.parse::<u32>() else {
        return false;
    };
    (1..=12).contains(&month) && (1..=31).contains(&day)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    #[test]
    fn builds_season_page_url() {
        let url = YucClient::season_page_url(2026, 4).expect("url");

        assert_eq!(url, "https://yuc.wiki/202604/");
    }

    #[test]
    fn rejects_non_season_month() {
        let error = YucClient::season_page_url(2026, 5).expect_err("invalid");

        assert!(error.to_string().contains("season month"));
    }

    #[test]
    fn maps_dates_to_correct_season_month() {
        assert_eq!(
            YucClient::season_of_date(NaiveDate::from_ymd_opt(2026, 1, 1).expect("date")),
            (2026, 1)
        );
        assert_eq!(
            YucClient::season_of_date(NaiveDate::from_ymd_opt(2026, 3, 31).expect("date")),
            (2026, 1)
        );
        assert_eq!(
            YucClient::season_of_date(NaiveDate::from_ymd_opt(2026, 4, 1).expect("date")),
            (2026, 4)
        );
        assert_eq!(
            YucClient::season_of_date(NaiveDate::from_ymd_opt(2026, 6, 30).expect("date")),
            (2026, 4)
        );
        assert_eq!(
            YucClient::season_of_date(NaiveDate::from_ymd_opt(2026, 7, 1).expect("date")),
            (2026, 7)
        );
        assert_eq!(
            YucClient::season_of_date(NaiveDate::from_ymd_opt(2026, 9, 30).expect("date")),
            (2026, 7)
        );
        assert_eq!(
            YucClient::season_of_date(NaiveDate::from_ymd_opt(2026, 10, 1).expect("date")),
            (2026, 10)
        );
        assert_eq!(
            YucClient::season_of_date(NaiveDate::from_ymd_opt(2026, 12, 31).expect("date")),
            (2026, 10)
        );
    }

    #[test]
    fn parses_real_yuc_fixture_page() {
        let html = fs::read_to_string(fixture_path("yuc_202604.html")).expect("read fixture");
        let url = Url::parse("https://yuc.wiki/202604/").expect("url");

        let page = parse_yuc_page(&url, &html).expect("parse yuc fixture");
        assert_eq!(page.season_code, "202604");
        assert!(page.page_url.contains("202604"));
        assert!(page.entries.len() > 40);
        assert!(page
            .entries
            .iter()
            .any(|entry| entry.title_zh.contains("淡岛百景")));
        assert!(page.entries.iter().any(|entry| {
            entry
                .title_zh
                .contains("最强职业不是勇者也不是贤者好像是鉴定士")
                && entry.image_url.is_some()
        }));
    }
}
