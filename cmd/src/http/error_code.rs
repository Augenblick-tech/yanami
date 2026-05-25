/// HTTP API 业务码定义。
///
/// 编码规则：
/// - `00-xxx` 为系统级通用码，最终以三位整型返回，例如成功 `200`
/// - `MM-xxx` 为业务模块码，前两位 `MM` 表示模块，后三位 `xxx` 表示错误语义
/// - 例如：用户模块 `10` 的密码错误 `401`，最终业务码为 `10401`
///
/// 模块划分：
/// - `00` 系统与协议层：认证、鉴权、网关、内部错误
/// - `10` 用户与认证：注册、登录、密码、注册码
/// - `20` 番剧：番剧目录、元数据、追踪状态
/// - `30` 协作：团队、成员、订阅空间
/// - `40` Feed：RSS 源、采集输入校验
/// - `60` 下载：下载器、qbit、保存路径
pub mod code {
    pub const NOT_FOUND: i32 = 404;
    pub const FORBIDDEN: i32 = 403;
    pub const BAD_GATEWAY: i32 = 502;
    pub const INTERNAL_ERROR: i32 = 500;

    pub const IDENTITY_INVALID_INPUT: i32 = make(MODULE_IDENTITY, 400);
    pub const IDENTITY_PASSWORD_MISMATCH: i32 = make(MODULE_IDENTITY, 401);
    pub const IDENTITY_NOT_FOUND: i32 = make(MODULE_IDENTITY, 404);
    pub const IDENTITY_ALREADY_EXISTS: i32 = make(MODULE_IDENTITY, 409);
    pub const IDENTITY_EXPIRED: i32 = make(MODULE_IDENTITY, 410);
    pub const IDENTITY_EXHAUSTED: i32 = make(MODULE_IDENTITY, 429);

    pub const ANIME_INVALID_INPUT: i32 = make(MODULE_ANIME, 400);
    pub const ANIME_NOT_FOUND: i32 = make(MODULE_ANIME, 404);

    pub const COLLABORATION_INVALID_INPUT: i32 = make(MODULE_COLLABORATION, 400);
    pub const COLLABORATION_NOT_FOUND: i32 = make(MODULE_COLLABORATION, 404);

    pub const FEED_INVALID_INPUT: i32 = make(MODULE_FEED, 400);
    pub const DELIVERY_INVALID_INPUT: i32 = make(MODULE_DELIVERY, 400);
    pub const DELIVERY_NOT_FOUND: i32 = make(MODULE_DELIVERY, 404);

    pub const SYSTEM_INVALID_INPUT: i32 = make(MODULE_SYSTEM, 400);
    pub const SYSTEM_NOT_FOUND: i32 = make(MODULE_SYSTEM, 404);

    const MODULE_SYSTEM: i32 = 0;
    const MODULE_IDENTITY: i32 = 10;
    const MODULE_ANIME: i32 = 20;
    const MODULE_COLLABORATION: i32 = 30;
    const MODULE_FEED: i32 = 40;
    const MODULE_DELIVERY: i32 = 60;

    const fn make(module: i32, suffix: i32) -> i32 {
        if module == MODULE_SYSTEM {
            suffix
        } else {
            module * 1000 + suffix
        }
    }
}

/// 根据领域不变量消息映射稳定业务码。
///
/// 这里集中维护“领域错误消息 -> API 业务码”的对照，避免在 handler 或 error
/// 出站逻辑里散落裸数字。
pub fn invariant_violation_code(message: &'static str) -> i32 {
    match message {
        "password does not match" | "wrong password" => code::IDENTITY_PASSWORD_MISMATCH,
        "username cannot be empty"
        | "registration code cannot be empty"
        | "password must be at least 6 characters"
        | "invalid user role" => code::IDENTITY_INVALID_INPUT,
        "username already exists" => code::IDENTITY_ALREADY_EXISTS,
        "registration code is expired" => code::IDENTITY_EXPIRED,
        "registration code is exhausted" => code::IDENTITY_EXHAUSTED,
        "user not found" | "admin user not found" => code::IDENTITY_NOT_FOUND,
        "registration code not found" => code::IDENTITY_NOT_FOUND,
        "anime not found" => code::ANIME_NOT_FOUND,
        "anime id must be positive"
        | "anime title original_ja cannot be empty"
        | "anime search_name cannot be empty"
        | "anime air_date cannot be empty"
        | "anime season must be positive"
        | "invalid entry"
        | "anime air date is invalid"
        | "release published time is invalid" => code::ANIME_INVALID_INPUT,
        "personal space already exists"
        | "user already joined another team"
        | "team member already exists"
        | "team owner cannot leave directly" => code::COLLABORATION_INVALID_INPUT,
        "personal space not found"
        | "team not found"
        | "team space not found"
        | "team member not found" => code::COLLABORATION_NOT_FOUND,
        "feed source id cannot be empty"
        | "feed source title cannot be empty"
        | "feed source full site url cannot be empty"
        | "feed source search url cannot be empty"
        | "feed source search url must contain placeholder"
        | "feed source kind/url combination is invalid"
        | "feed source kind/search_url combination is invalid"
        | "resource title cannot be empty"
        | "resource source url cannot be empty" => code::FEED_INVALID_INPUT,
        "download driver key cannot be empty"
        | "download driver key is invalid"
        | "relative save path cannot be empty"
        | "relative save path is invalid"
        | "relative save path is not valid utf-8"
        | "qbit endpoint cannot be empty"
        | "qbit endpoint is invalid"
        | "qbit username cannot be empty"
        | "qbit secret cannot be empty"
        | "qbit download path cannot be empty"
        | "qbit download path is invalid"
        | "qbit upload body missing boundary" => code::DELIVERY_INVALID_INPUT,
        "user qbit download profile not found" => code::DELIVERY_NOT_FOUND,
        "subscription space not found" => code::SYSTEM_NOT_FOUND,
        _ => code::SYSTEM_INVALID_INPUT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_keeps_system_codes_short_and_prefixes_business_codes() {
        assert_eq!(code::IDENTITY_PASSWORD_MISMATCH, 10401);
        assert_eq!(code::ANIME_NOT_FOUND, 20404);
        assert_eq!(code::COLLABORATION_NOT_FOUND, 30404);
        assert_eq!(code::DELIVERY_INVALID_INPUT, 60400);
    }

    #[test]
    fn invariant_violation_code_maps_known_identity_messages() {
        assert_eq!(
            invariant_violation_code("password does not match"),
            code::IDENTITY_PASSWORD_MISMATCH
        );
        assert_eq!(
            invariant_violation_code("wrong password"),
            code::IDENTITY_PASSWORD_MISMATCH
        );
        assert_eq!(
            invariant_violation_code("username already exists"),
            code::IDENTITY_ALREADY_EXISTS
        );
        assert_eq!(
            invariant_violation_code("registration code is exhausted"),
            code::IDENTITY_EXHAUSTED
        );
    }

    #[test]
    fn invariant_violation_code_maps_domain_specific_messages() {
        assert_eq!(
            invariant_violation_code("anime not found"),
            code::ANIME_NOT_FOUND
        );
        assert_eq!(
            invariant_violation_code("team member not found"),
            code::COLLABORATION_NOT_FOUND
        );
        assert_eq!(
            invariant_violation_code("feed source search url must contain placeholder"),
            code::FEED_INVALID_INPUT
        );
        assert_eq!(
            invariant_violation_code("qbit endpoint is invalid"),
            code::DELIVERY_INVALID_INPUT
        );
        assert_eq!(
            invariant_violation_code("subscription space not found"),
            code::SYSTEM_NOT_FOUND
        );
        assert_eq!(
            invariant_violation_code("totally unknown"),
            code::SYSTEM_INVALID_INPUT
        );
    }
}
