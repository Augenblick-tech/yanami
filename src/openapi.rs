use anna::{anime::anime::AnimeInfo, qbit::qbit::QbitConfig};
use utoipa::{openapi::security::{ApiKey, ApiKeyValue, SecurityScheme}, Modify, OpenApi};

use crate::{
    common::{
        auth::UserCharacter,
        errors::Error,
        result::{JsonResultAuthBody, JsonResultDownloadPath, JsonResultRSS, JsonResultVecAnimeInfo, JsonResultVecAnimeRssRecord, JsonResultVecAnimeStatus, JsonResultVecGroupRule, JsonResultVecRSS, JsonResultVecUserEntity, JsonResulti32},
    },
    models::{
        anime::{AnimeRecordReq, AnimeStatus}, config::ServiceConfig, rss::{AnimeRssRecord, DelRSS, RSSReq, RSS}, rule::{DelRule, GroupRule, Rule}, user::{AuthBody, LoginReq, RegisterCodeReq, RegisterCodeRsp, RegisterReq, UserEntity}
    },
};


    #[derive(OpenApi)]
    #[openapi(
        modifiers(&SecurityAddon),
        paths(
            crate::hander::user::login,
            crate::hander::user::register,
            crate::hander::user::register_code,
            crate::hander::user::users,
            crate::hander::rss::rss_list,
            crate::hander::rss::set_rss,
            crate::hander::rss::del_rss,
            crate::hander::rule::set_rule,
            crate::hander::rule::del_rule,
            crate::hander::rule::rules,
            crate::hander::config::set_config,
            crate::hander::config::get_config,
            crate::hander::anime::animes,
            crate::hander::anime::set_anime,
            crate::hander::anime::anime_records,
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
                JsonResultVecGroupRule,
                GroupRule,
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
