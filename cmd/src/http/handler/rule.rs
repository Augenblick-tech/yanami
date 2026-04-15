use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use domain::rule::MatchingRuleId;

use crate::http::{error::ApiError, model::*, state::AppState};

/// 查询空间下的规则集。
#[utoipa::path(
    get,
    path = "/api/v1/space/rules",
    security(("bearer_auth" = [])),
    responses((status = 200, description = "规则查询成功。"))
)]
pub async fn get_rules(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<RulesResponse>>, ApiError> {
    let outcome = state.rule_service.get_rules(state.admin_space_id).await?;
    Ok(Json(ApiResponse::ok(RulesResponse {
        owner_id: outcome.space_id.0,
        rules: outcome.rules.into_iter().map(Into::into).collect(),
    })))
}

/// 保存空间下的规则集。
#[utoipa::path(
    post,
    path = "/api/v1/space/rules",
    security(("bearer_auth" = [])),
    request_body = MatchingRuleRequest,
    responses((status = 200, description = "保存成功，返回规则。"))
)]
pub async fn create_rule(
    State(state): State<Arc<AppState>>,
    Json(request): Json<MatchingRuleRequest>,
) -> Result<Json<ApiResponse<MatchingRuleView>>, ApiError> {
    let id = request.name.clone();
    let outcome = state
        .rule_service
        .save_rule(state.admin_space_id, request.into_domain(id))
        .await?;
    Ok(Json(ApiResponse::ok(outcome.rule.into())))
}

/// 修改单条空间规则。
#[utoipa::path(
    put,
    path = "/api/v1/space/rules/{rule_id}",
    security(("bearer_auth" = [])),
    params(("rule_id" = String, Path, description = "规则标识")),
    request_body = MatchingRuleRequest,
    responses((status = 200, description = "修改成功，返回规则。"))
)]
pub async fn update_rule(
    State(state): State<Arc<AppState>>,
    Path(rule_id): Path<String>,
    Json(request): Json<MatchingRuleRequest>,
) -> Result<Json<ApiResponse<MatchingRuleView>>, ApiError> {
    let outcome = state
        .rule_service
        .save_rule(state.admin_space_id, request.into_domain(rule_id))
        .await?;
    Ok(Json(ApiResponse::ok(outcome.rule.into())))
}

/// 失活单条空间规则。
#[utoipa::path(
    delete,
    path = "/api/v1/space/rules/{rule_id}",
    security(("bearer_auth" = [])),
    params(("rule_id" = String, Path, description = "规则标识")),
    responses((status = 200, description = "规则已失活。"))
)]
pub async fn delete_rule(
    State(state): State<Arc<AppState>>,
    Path(rule_id): Path<String>,
) -> Result<Json<ApiResponse<DeleteMatchingRuleResponse>>, ApiError> {
    let outcome = state
        .rule_service
        .delete_rule(state.admin_space_id, MatchingRuleId(rule_id))
        .await?;
    Ok(Json(ApiResponse::ok(DeleteMatchingRuleResponse {
        id: outcome.rule_id.0,
    })))
}
