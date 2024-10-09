use anna::{anime::tracker::AnimeInfo, qbit::qbitorrent::QbitConfig};
use utoipa::{openapi::security::{ApiKey, ApiKeyValue, SecurityScheme}, Modify, OpenApi};

use crate::{
    common::{
        auth::UserCharacter,
        errors::Error,
        result::{JsonResultAuthBody, JsonResultDownloadPath, JsonResultRSS, JsonResultVecAnimeInfo, JsonResultVecAnimeRssRecord, JsonResultVecAnimeStatus, JsonResultVecRule, JsonResultVecRSS, JsonResultVecUserEntity, JsonResulti32},
    },
    models::{
        anime::{AnimeRecordReq, AnimeStatus}, config::ServiceConfig, rss::{AnimeRssRecord, DelRSS, RSSReq, RSS}, rule::{DelRule, Rule}, user::{AuthBody, LoginReq, RegisterCodeReq, RegisterCodeRsp, RegisterReq, UserEntity}
    },
};


    #[derive(OpenApi)]
    #[openapi(
        modifiers(&SecurityAddon),
        paths(
            crate::handler::user::login,
            crate::handler::user::register,
            crate::handler::user::register_code,
            crate::handler::user::users,
            crate::handler::rss::rss_list,
            crate::handler::rss::set_rss,
            crate::handler::rss::del_rss,
            crate::handler::rule::set_rule,
            crate::handler::rule::del_rule,
            crate::handler::rule::rules,
            crate::handler::config::set_config,
            crate::handler::config::get_config,
            crate::handler::anime::animes,
            crate::handler::anime::set_anime,
            crate::handler::anime::anime_records,
        ),
        components(
            schemas(
                UserCharacter,
                UserEntity, 
                Error, 
                AuthBody, 
                RSS, 
                RSSReq,
                DelRSS,
                JsonResultAuthBody,
                JsonResultVecUserEntity,
                JsonResultVecRSS,
                JsonResultRSS,
                RegisterCodeReq,
                RegisterCodeRsp,
                RegisterReq,
                JsonResulti32,
                LoginReq,
                JsonResultVecRule,
                Rule,
                DelRule,
                JsonResultDownloadPath,
                ServiceConfig,
                JsonResultVecAnimeInfo,
                JsonResultVecAnimeStatus,
                AnimeInfo,
                AnimeStatus,
                AnimeRecordReq,
                AnimeRssRecord,
                JsonResultVecAnimeRssRecord,
                QbitConfig,
            )
        ),
    )]
    pub struct ApiDoc;

    struct SecurityAddon;

    impl Modify for SecurityAddon {
        fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
            if let Some(components) = openapi.components.as_mut() {
                components.add_security_scheme(
                    "api_key",
                    SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("Authorization"))),
                )
            }
        }
    }
