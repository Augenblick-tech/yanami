use utoipa::{openapi::security::{ApiKey, ApiKeyValue, SecurityScheme}, Modify, OpenApi};

use crate::{
    common::{
        auth::UserCharacter,
        errors::Error,
        result::{JsonResultAuthBody, JsonResultRSS, JsonResultVecRSS, JsonResultVecUserEntity, JsonResulti32},
    },
    models::{
        rss::{DelRSS, RSSReq, RSS},
        user::{AuthBody, LoginReq, RegisterCodeReq, RegisterCodeRsp, RegisterReq, UserEntity},
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
            )
        ),
        tags(
            (name = "yanami", description = "yanami management API")
        )
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
