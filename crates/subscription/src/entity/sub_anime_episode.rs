use std::{collections::HashMap, sync::Arc};

use common::shared::error::Error;

use crate::entity::{
    cap::SubAnimeRepository,
    episode_entity::EpsiodeEntity,
    model::{Episode, MatchedEpisode},
    sub_anime_entity::SubAnimeEntity,
};

#[derive(Clone)]
pub struct SubAnimeEpsiodes {
    sub_anime_id: i64,
    eps: u32,
    repo: Arc<dyn SubAnimeRepository>,
}

impl SubAnimeEpsiodes {
    pub(super) fn new(sub_anime_id: i64, eps: u32, repo: Arc<dyn SubAnimeRepository>) -> Self {
        Self {
            sub_anime_id,
            eps,
            repo,
        }
    }
}

impl SubAnimeEpsiodes {
    pub async fn list(&self) -> Result<Vec<EpsiodeEntity>, Error> {
        Ok(self
            .repo
            .list_eps(self.sub_anime_id)
            .await
            .map_err(|e| Error::external("sub anime eps get epsiode failed", e))?
            .into_iter()
            .map(|i| EpsiodeEntity::new(i.data, i.extend))
            .collect())
    }

    pub(super) async fn save_eps(
        &self,
        rule_id: i64,
        eps: Vec<MatchedEpisode>,
    ) -> Result<(), Error> {
        let prop = self
            .repo
            .find_sub_anime(self.sub_anime_id)
            .await
            .map_err(|e| Error::external("sub anime eps load entity failed", e))?
            .ok_or_else(|| Error::not_found("sub anime not found"))?;
        let mut entity = SubAnimeEntity::new(prop.data, prop.extend);
        // 尝试绑定规则
        entity.auto_bind_rule(rule_id)?;

        // 计算剧集编号
        let entity_eps = self.list().await?;
        let mut entity_eps_matched = entity_eps
            .iter()
            .map(|i| MatchedEpisode {
                sub_anime_id: i.sub_anime_id(),
                resource_id: *i.resource_id(),
                title: i.title().into(),
                status: i.status(),
            })
            .collect::<Vec<_>>();
        for i in eps {
            if !entity_eps_matched
                .iter()
                .any(|item| item.resource_id == i.resource_id)
            {
                tracing::info!(
                    "sub anime matcher matched resource, sub_anime_id: {}, resource title: {}",
                    i.sub_anime_id,
                    i.title
                );
                entity_eps_matched.push(i);
            }
        }
        let eps_map = extract_episode_number(&entity_eps_matched);
        let new_eps = entity_eps_matched
            .into_iter()
            .map(|i| {
                let num = eps_map.get(&i.resource_id).copied();
                Episode {
                    sub_anime_id: i.sub_anime_id,
                    resource_id: i.resource_id,
                    status: i.status,
                    ep_num: num,
                }
            })
            .collect::<Vec<_>>();
        entity.update_progress(&new_eps);
        self.repo
            .update_sub_anime_progress(entity.get_base_data(), &new_eps)
            .await
            .map_err(|e| Error::external("sub anime eps update progress failed", e))
    }

    pub async fn check_missing_episodes(&self) -> Result<bool, Error> {
        let eps = self
            .repo
            .list_eps(self.sub_anime_id)
            .await
            .map_err(|e| Error::external("sub anime eps get epsiode failed", e))?
            .into_iter()
            .filter_map(|i| i.data.ep.ep_num.map(|v| v as i64))
            .collect::<Vec<_>>();

        Ok(check_missing_episodes(&eps, self.eps))
    }

    pub async fn binding_rule(&self, rule_id: i64) -> Result<(), Error> {
        let prop = self
            .repo
            .find_sub_anime(self.sub_anime_id)
            .await
            .map_err(|e| Error::external("sub anime eps load entity failed", e))?
            .ok_or_else(|| Error::not_found("sub anime not found"))?;
        let mut entity = SubAnimeEntity::new(prop.data, prop.extend);
        let Some(binded_rule_id) = entity.get_rule_id() else {
            entity.auto_bind_rule(rule_id)?;
            self.repo
                .update_sub_anime(entity.get_base_data())
                .await
                .map_err(|e| Error::external("sub anime eps bind rule save entity failed", e))?;
            return Ok(());
        };
        if binded_rule_id == rule_id {
            return Ok(());
        }

        // 清空现有的剧集并同步更新规则
        self.repo
            .binding_rule_and_clear_eps(self.sub_anime_id, rule_id)
            .await
            .map_err(|e| Error::external("sub anime eps bind rule clear eps failed", e))?;

        Ok(())
    }
}

fn check_missing_episodes(episodes: &[i64], total_episodes: u32) -> bool {
    if episodes.len() < 2 {
        return false; // 无内部可比对，不算漏集
    }

    let mut missing = 0;

    for i in 0..episodes.len() - 1 {
        let diff = episodes[i + 1] - episodes[i];
        if diff > 1 {
            missing += diff - 1;
        }
    }

    missing > 0 && missing <= total_episodes as i64
}

fn extract_episode_number(eps: &[MatchedEpisode]) -> HashMap<[u8; 20], f64> {
    // 1. 解析每个标题中的所有数字
    let number_lists: Vec<Vec<f64>> = eps
        .iter()
        .map(|ep| {
            let mut nums = Vec::new();
            let mut current = String::new();
            for ch in ep.title.chars() {
                if ch.is_ascii_digit() || ch == '.' {
                    current.push(ch);
                } else {
                    if !current.is_empty() {
                        if let Ok(num) = current.parse::<f64>() {
                            nums.push(num);
                        }
                        current.clear();
                    }
                }
            }
            if !current.is_empty()
                && let Ok(num) = current.parse::<f64>()
            {
                nums.push(num);
            }
            nums
        })
        .collect();

    // 无有效数字，返回空
    if number_lists.is_empty() {
        return HashMap::new();
    }

    let min_len = number_lists.iter().map(|l| l.len()).min().unwrap_or(0);
    if min_len == 0 {
        return HashMap::new();
    }

    // 2. 构建每一列（只比较所有资源共有的下标）
    let columns: Vec<Vec<f64>> = (0..min_len)
        .map(|i| number_lists.iter().map(|l| l[i]).collect())
        .collect();

    // 3. 选择最佳列作为剧集列
    let best_col_index = match find_best_column(&columns) {
        Some(idx) => idx,
        None => return HashMap::new(),
    };

    // 4. 构建 resource_id → 剧集数 的映射
    let mut result = HashMap::new();
    for (ep, nums) in eps.iter().zip(number_lists.iter()) {
        if let Some(&value) = nums.get(best_col_index) {
            result.insert(ep.resource_id, value);
        }
    }

    result
}

/// 从所有列中选出最可能是剧集编号的那一列
fn find_best_column(columns: &[Vec<f64>]) -> Option<usize> {
    let mut best_idx = None;
    let mut best_step = f64::MAX;
    let mut best_dup = usize::MAX;

    for (idx, col) in columns.iter().enumerate() {
        if col.is_empty() {
            continue;
        }

        // 去重排序，计算递增步长（唯一值相邻差的最小值）
        let mut uniq: Vec<f64> = col.to_vec();
        uniq.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        uniq.dedup();

        let step = if uniq.len() >= 2 {
            uniq.windows(2)
                .map(|w| w[1] - w[0])
                .filter(|&d| d > 0.0)
                .fold(f64::MAX, |a, b| a.min(b))
        } else {
            f64::MAX // 无递增
        };

        // 计算重复数（原始列中出现次数最多的值的频次）
        let mut freq_map: HashMap<u64, usize> = HashMap::new();
        for &num in col {
            // 用位模式作为键，避免浮点数比较问题（假设没有 NaN）
            let key = num.to_bits();
            *freq_map.entry(key).or_insert(0) += 1;
        }
        let dup = freq_map.values().max().copied().unwrap_or(0);

        // 优先递增步长小的，步长相等时取重复数少的
        let is_better = if step < best_step {
            true
        } else if (step - best_step).abs() < 1e-9 {
            dup < best_dup
        } else {
            false
        };

        if is_better {
            best_step = step;
            best_dup = dup;
            best_idx = Some(idx);
        }
    }

    best_idx
}
