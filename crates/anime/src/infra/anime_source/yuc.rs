use std::collections::{HashMap, HashSet};
use std::time::Duration;

use anyhow::{Result, anyhow};
use chrono::{Datelike, Local, NaiveDate};
use reqwest::{Client, Url};
use scraper::{ElementRef, Html, Selector};

/// YUC 季度日历页面（对外公开）
#[derive(Debug, Clone)]
pub struct SeasonCalendarPage {
    pub page_title: String,
    pub page_url: String,
    pub season_code: String,
    pub updated_at: Option<String>,
    pub entries: Vec<AnimeEntry>,
}

/// YUC 番剧条目（对外公开）
#[derive(Debug, Clone)]
pub struct AnimeEntry {
    pub title_zh: String,
    pub title_original: Option<String>,
    pub image_url: Option<String>,
    pub source_type: Option<String>,
    pub genre_text: Option<String>,
    pub original_work: Option<String>,
    pub original_work_extra: Option<String>,
    pub schedule_weekday: Option<String>,
    pub schedule_time: Option<String>,
    pub schedule_start_date: Option<String>,
    pub schedule_note: Option<String>,
    pub broadcast_text: Option<String>,
    pub streaming_links: Vec<Link>,
    pub resource_links: Vec<Link>,
    pub staff: Vec<LabeledValue>,
    pub cast: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Link {
    pub label: String,
    pub href: String,
}

#[derive(Debug, Clone)]
pub struct LabeledValue {
    pub label: String,
    pub value: String,
}

#[derive(Debug)]
pub struct YucClient {
    client: Client,
}

impl YucClient {
    pub fn new() -> Result<Self, anyhow::Error> {
        Ok(Self {
            client: Client::builder()
                .user_agent("yanami yuc infrastructure")
                .timeout(Duration::from_secs(30))
                .build()?,
        })
    }

    pub fn season_of_date(date: NaiveDate) -> Result<(i32, u32)> {
        match date.month() {
            1..=3 => Ok((date.year(), 1)),
            4..=6 => Ok((date.year(), 4)),
            7..=9 => Ok((date.year(), 7)),
            10..=12 => Ok((date.year(), 10)),
            _ => Err(anyhow!("month must be 1..=12")),
        }
    }

    pub fn season_page_url(year: i32, month: u32) -> Result<String, anyhow::Error> {
        if !matches!(month, 1 | 4 | 7 | 10) {
            return Err(anyhow::anyhow!("season month must be 1, 4, 7, or 10"));
        }
        Ok(format!("https://yuc.wiki/{year}{month:02}/"))
    }

    pub async fn fetch_current_season_calendar(&self) -> Result<SeasonCalendarPage> {
        let today = Local::now().date_naive();
        let (year, month) = Self::season_of_date(today)?;
        self.fetch_season_calendar(year, month).await
    }

    pub async fn fetch_season_calendar(&self, year: i32, month: u32) -> Result<SeasonCalendarPage> {
        let page_url = Self::season_page_url(year, month)?;
        self.fetch_by_url(&page_url).await
    }

    pub async fn fetch_by_url(&self, page_url: &str) -> Result<SeasonCalendarPage> {
        let url = Url::parse(page_url)?;
        let html = self.client.get(url.clone()).send().await?.text().await?;
        parse_page(&url, &html)
    }
}

// HTML 解析逻辑

#[derive(Debug, Clone)]
struct SchedulePreview {
    time: Option<String>,
    start_date: Option<String>,
    note: Option<String>,
    image_url: Option<String>,
    streaming_links: Vec<Link>,
}

fn parse_page(base_url: &Url, html: &str) -> Result<SeasonCalendarPage, anyhow::Error> {
    let document = Html::parse_document(html);
    let root = select_root(&document).ok_or_else(|| anyhow::anyhow!("yuc content root missing"))?;
    let page_title =
        extract_title(&document).ok_or_else(|| anyhow::anyhow!("yuc page title missing"))?;
    let updated_at = extract_updated(&document);
    let season_code = extract_code(base_url)?;
    let previews = parse_previews(base_url, &root)?;
    let entries = parse_entries(base_url, &root, &previews)?;
    Ok(SeasonCalendarPage {
        page_title,
        page_url: base_url.as_str().to_string(),
        season_code,
        updated_at,
        entries,
    })
}

fn select_root<'a>(doc: &'a Html) -> Option<ElementRef<'a>> {
    for sel in &[
        ".post-body",
        "article.post-block",
        "body",
        "main",
        "article",
    ] {
        if let Some(el) = doc.select(&Selector::parse(sel).ok()?).next() {
            return Some(el);
        }
    }
    None
}

fn extract_title(doc: &Html) -> Option<String> {
    let sel = Selector::parse("h1.post-title, title").ok()?;
    doc.select(&sel)
        .next()
        .map(|el| clean(&el.text().collect::<Vec<_>>().join(" ")))
        .filter(|s| !s.is_empty())
}

fn extract_updated(doc: &Html) -> Option<String> {
    let sel = Selector::parse(r#"time[itemprop="dateModified"]"#).ok()?;
    doc.select(&sel)
        .next()
        .map(|el| clean(&el.text().collect::<Vec<_>>().join(" ")))
        .filter(|s| !s.is_empty())
}

fn extract_code(url: &Url) -> Result<String, anyhow::Error> {
    let seg = url
        .path_segments()
        .and_then(|mut s| s.rfind(|seg| !seg.is_empty()))
        .ok_or_else(|| anyhow::anyhow!("season code missing"))?;
    if seg.len() == 6 && seg.chars().all(|c| c.is_ascii_digit()) {
        Ok(seg.to_string())
    } else {
        Err(anyhow::anyhow!("invalid season code: {seg}"))
    }
}

fn parse_previews(
    base_url: &Url,
    root: &ElementRef<'_>,
) -> Result<HashMap<String, SchedulePreview>, anyhow::Error> {
    let float_sel = s("div[style=\"float:left\"]")?;
    let date_sel = s(".div_date")?;
    let title_sel = s("td[class^=\"date_title_\"]")?;
    let time_sel = s("p.imgtext4, p.imgtext5")?;
    let date_p_sel = s("p.imgep, p.imgep2")?;
    let link_sel = s("tr.tr_area a")?;
    let img_sel = s(".div_date img")?;

    let mut map = HashMap::new();
    for block in root.select(&float_sel) {
        if block.select(&date_sel).next().is_none() {
            continue;
        }
        let Some(tzh) = first_title(&block, &title_sel) else {
            continue;
        };

        let mut start = None;
        let mut note = None;
        for item in block.select(&date_p_sel) {
            let t = clean(&item.text().collect::<Vec<_>>().join(" "));
            if t.is_empty() {
                continue;
            }
            if looks_date(&t) {
                start = Some(t);
            } else {
                note = Some(t);
            }
        }

        let mut links = Vec::new();
        for a in block.select(&link_sel) {
            if let Some(l) = parse_link(base_url, &a)? {
                links.push(l);
            }
        }
        dedup_links(&mut links);

        map.insert(
            normalize(&tzh),
            SchedulePreview {
                time: first_text(&block, &time_sel),
                start_date: start,
                note,
                image_url: block
                    .select(&img_sel)
                    .next()
                    .and_then(|img| image_url(base_url, &img).transpose())
                    .transpose()?,
                streaming_links: links,
            },
        );
    }
    Ok(map)
}

fn parse_entries(
    base_url: &Url,
    root: &ElementRef<'_>,
    previews: &HashMap<String, SchedulePreview>,
) -> Result<Vec<AnimeEntry>, anyhow::Error> {
    let tzh_sel = s("p[class^=\"title_cn_r\"]")?;
    let tjp_sel = s("p[class^=\"title_jp_r\"]")?;
    let type_sel = s(".type_a_r, .type_b_r, .type_c_r, .type_e_r")?;
    let genre_sel = s(".type_tag_r")?;
    let staff_sel = s(".staff_r, .staff_r1, .staff_r2")?;
    let cast_sel = s(".cast_r")?;
    let rlink_sel = s(".link_a_r a, .link_b_r a")?;
    let bc_sel = s(".broadcast_r")?;
    let bcx_sel = s(".broadcast_ex_r")?;

    let mut entries = Vec::new();

    for tzh_el in root.select(&tzh_sel) {
        // 先干净地把文本 collect 成 String
        let tzh: String = tzh_el
            .text()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        // 顶替掉之前错误的 let Some(...) else 结构
        if tzh.is_empty() {
            continue;
        }

        let mut parent = tzh_el.parent();
        let mut table_el = None;
        while let Some(node) = parent {
            if let scraper::node::Node::Element(el) = node.value()
                && el.name() == "table" {
                    table_el = ElementRef::wrap(node);
                    break;
                }
            parent = node.parent();
        }

        let Some(table) = table_el else {
            continue;
        };
        let prev = previews.get(&normalize(&tzh));

        let mut img_url = None;
        if let Some(parent_node) = table.parent().and_then(ElementRef::wrap) {
            let local_img_sel = s("img")?;
            if let Some(img) = parent_node.select(&local_img_sel).next() {
                img_url = image_url(base_url, &img)?;
            }
        }
        let final_img_url = img_url.or_else(|| prev.and_then(|p| p.image_url.clone()));

        let mut staff = Vec::new();
        let mut orig_work = None;
        let mut orig_extra = None;
        if let Some(cell) = table.select(&staff_sel).next() {
            for line in extract_lines(&cell) {
                if let Some((l, v)) = line.split_once('：').or_else(|| line.split_once(':')) {
                    let (lbl, val) = (l.trim().to_string(), v.trim().to_string());
                    if lbl == "原作" || lbl == "原案" {
                        orig_work = Some(val.clone());
                    }
                    staff.push(LabeledValue {
                        label: lbl,
                        value: val,
                    });
                } else if line.starts_with('(') && line.ends_with(')') {
                    orig_extra = Some(line);
                } else if let Some(last) = staff.last_mut() {
                    if !last.value.is_empty() {
                        last.value.push(' ');
                    }
                    last.value.push_str(&line);
                }
            }
        }

        let cast: Vec<String> = table
            .select(&cast_sel)
            .next()
            .map(|c| extract_lines(&c))
            .unwrap_or_default();
        let mut rlinks = Vec::new();
        for a in table.select(&rlink_sel) {
            if let Some(l) = parse_link(base_url, &a)? {
                rlinks.push(l);
            }
        }
        dedup_links(&mut rlinks);

        let mut bc_text = first_text(&table, &bc_sel);
        if let Some(extra) = first_text(&table, &bcx_sel).filter(|t| !t.is_empty()) {
            match &mut bc_text {
                Some(t) if !t.is_empty() => {
                    t.push(' ');
                    t.push_str(&extra);
                }
                None => bc_text = Some(extra),
                _ => {}
            }
        }

        let weekday = bc_text.as_ref().and_then(|t| {
            ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
                .iter()
                .find(|&&w| t.contains(w))
                .map(|&w| w.to_string())
        });

        entries.push(AnimeEntry {
            title_zh: tzh, // 这里现在是纯正的 String 了
            title_original: first_title(&table, &tjp_sel),
            image_url: final_img_url,
            source_type: first_text(&table, &type_sel),
            genre_text: first_text(&table, &genre_sel),
            original_work: orig_work,
            original_work_extra: orig_extra,
            schedule_weekday: weekday,
            schedule_time: prev.and_then(|p| p.time.clone()),
            schedule_start_date: prev.and_then(|p| p.start_date.clone()),
            schedule_note: prev.and_then(|p| p.note.clone()),
            broadcast_text: bc_text,
            streaming_links: prev.map(|p| p.streaming_links.clone()).unwrap_or_default(),
            resource_links: rlinks,
            staff,
            cast,
        });
    }
    Ok(entries)
}

fn s(raw: &str) -> Result<Selector, anyhow::Error> {
    Selector::parse(raw).map_err(|e| anyhow::anyhow!("invalid selector `{raw}`: {e}"))
}

fn clean(text: &str) -> String {
    text.replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn first_text(el: &ElementRef<'_>, sel: &Selector) -> Option<String> {
    el.select(sel)
        .next()
        .map(|e| clean(&e.text().collect::<Vec<_>>().join(" ")))
        .filter(|s| !s.is_empty())
}

fn first_title(el: &ElementRef<'_>, sel: &Selector) -> Option<String> {
    el.select(sel)
        .next()
        .map(|e| {
            e.text()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<String>()
        })
        .filter(|s| !s.is_empty())
}

fn image_url(base: &Url, img: &ElementRef<'_>) -> Result<Option<String>, anyhow::Error> {
    let href = img
        .value()
        .attr("data-src")
        .or_else(|| img.value().attr("src"))
        .or_else(|| img.value().attr("data-lazy-src"));
    match href {
        Some(h) => Ok(Some(resolve(base, h)?)),
        None => Ok(None),
    }
}

fn parse_link(base: &Url, a: &ElementRef<'_>) -> Result<Option<Link>, anyhow::Error> {
    let href = match a.value().attr("href") {
        Some(h) => h,
        None => return Ok(None),
    };
    let label = clean(&a.text().collect::<Vec<_>>().join(" "));
    if label.is_empty() {
        return Ok(None);
    }
    Ok(Some(Link {
        label,
        href: resolve(base, href)?,
    }))
}

fn resolve(base: &Url, href: &str) -> Result<String, anyhow::Error> {
    base.join(href)
        .map(|u| u.to_string())
        .map_err(|e| anyhow::anyhow!("link resolve {href}: {e}"))
}

fn dedup_links(links: &mut Vec<Link>) {
    let mut seen = HashSet::new();
    links.retain(|l| seen.insert((l.label.clone(), l.href.clone())));
}

fn extract_lines(root: &ElementRef<'_>) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for child in root.children() {
        if let Some(t) = child.value().as_text() {
            let t = clean(t);
            if t.is_empty() {
                continue;
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(&t);
            continue;
        }
        let Some(el) = ElementRef::wrap(child) else {
            continue;
        };
        if el.value().name() == "br" {
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            continue;
        }
        let t = clean(&el.text().collect::<Vec<_>>().join(" "));
        if t.is_empty() {
            continue;
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(&t);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

fn looks_date(text: &str) -> bool {
    let val = text.strip_suffix('~').unwrap_or(text);
    let Some((m, d)) = val.split_once('/') else {
        return false;
    };
    let (Ok(m), Ok(d)) = (m.parse::<u32>(), d.parse::<u32>()) else {
        return false;
    };
    (1..=12).contains(&m) && (1..=31).contains(&d)
}

fn normalize(title: &str) -> String {
    title.chars().filter(|ch| !ch.is_whitespace()).collect()
}
