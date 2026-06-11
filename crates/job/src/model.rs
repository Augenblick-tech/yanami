use std::sync::{Arc, atomic::AtomicBool};
use std::time::Duration;

// 任务的配置信息
#[derive(Clone, Debug)]
pub struct TaskConfig {
    pub name: String,
    pub interval: Duration,
    // 是否允许重入
    pub allow_reentry: bool,
}

// 内部任务包装器
pub(super) struct Task {
    pub config: TaskConfig,
    // 闭包约束：接收 Arc<Boss>，返回一个异步的 Future (用 BoxPin 擦除类型)
    pub handler: Box<dyn Fn() -> futures::future::BoxFuture<'static, ()> + Send + Sync>,
    // 重入锁：无锁并发，极致轻量
    pub is_running: Arc<AtomicBool>,
}
