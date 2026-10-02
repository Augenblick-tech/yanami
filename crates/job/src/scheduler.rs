use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::task::JoinHandle;

use crate::model::{Task, TaskConfig};

pub struct TaskScheduler {
    tasks: Vec<Task>,
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskScheduler {
    pub fn new() -> Self {
        Self { tasks: Vec::new() }
    }

    pub fn register<F, Fut>(&mut self, config: TaskConfig, handler: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        self.tasks.push(Task {
            config,
            is_running: Arc::new(AtomicBool::new(false)),
            handler: Box::new(move || Box::pin(handler())),
        });
    }
}

impl TaskScheduler {
    pub fn start(self) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::with_capacity(self.tasks.len());

        for task in self.tasks {
            let handle = tokio::spawn(async move {
                // tokio::time::interval 会补偿执行时间，保证绝对的周期触发，不会像 sleep 那样越跑越偏
                let mut ticker = tokio::time::interval(task.config.interval);

                loop {
                    ticker.tick().await;

                    // 防重入校验
                    if !task.config.allow_reentry
                        && task
                            .is_running
                            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                            .is_err()
                    {
                        continue;
                    }

                    //  准备执行上下文
                    let is_running = task.is_running.clone();
                    let allow_reentry = task.config.allow_reentry;
                    let handler = &task.handler;

                    //  生成执行 Future
                    let run_future = handler();

                    //  开启独立协程执行业务
                    // 保证业务的执行不会阻塞外层的 interval 计时器
                    tokio::spawn(async move {
                        // 执行真正的业务逻辑
                        run_future.await;

                        // 业务执行完毕，释放重入锁
                        if !allow_reentry {
                            is_running.store(false, Ordering::Release);
                        }
                    });
                }
            });

            handles.push(handle);
        }

        handles
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    // 记录 handler 的执行状态：当前并发数、历史最大并发数、启动次数
    struct ConcurrencyProbe {
        in_flight: Arc<AtomicUsize>,
        max_in_flight: Arc<AtomicUsize>,
        starts: Arc<AtomicUsize>,
    }

    impl ConcurrencyProbe {
        fn new() -> Self {
            Self {
                in_flight: Arc::new(AtomicUsize::new(0)),
                max_in_flight: Arc::new(AtomicUsize::new(0)),
                starts: Arc::new(AtomicUsize::new(0)),
            }
        }

        // 生成一个每次执行都会统计并发数、并占用 handle_duration 的 handler
        fn handler(
            &self,
            handle_duration: Duration,
        ) -> impl Fn() -> futures::future::BoxFuture<'static, ()> + Send + Sync + 'static {
            let in_flight = self.in_flight.clone();
            let max_in_flight = self.max_in_flight.clone();
            let starts = self.starts.clone();
            move || {
                let in_flight = in_flight.clone();
                let max_in_flight = max_in_flight.clone();
                let starts = starts.clone();
                Box::pin(async move {
                    let current = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    max_in_flight.fetch_max(current, Ordering::SeqCst);
                    starts.fetch_add(1, Ordering::SeqCst);

                    tokio::time::sleep(handle_duration).await;

                    in_flight.fetch_sub(1, Ordering::SeqCst);
                })
            }
        }

        fn max_in_flight(&self) -> usize {
            self.max_in_flight.load(Ordering::SeqCst)
        }

        fn starts(&self) -> usize {
            self.starts.load(Ordering::SeqCst)
        }
    }

    fn config(name: &str, allow_reentry: bool) -> TaskConfig {
        TaskConfig {
            name: name.to_string(),
            interval: Duration::from_millis(20),
            allow_reentry,
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_disallow_reentry_never_runs_handler_concurrently() {
        let probe = ConcurrencyProbe::new();

        let mut scheduler = TaskScheduler::new();
        scheduler.register(config("no-reentry", false), probe.handler(Duration::from_millis(200)));

        let handles = scheduler.start();
        assert_eq!(handles.len(), 1);

        // 200ms 的单次执行时长 vs 20ms 的调度间隔：若允许重入会并发多次
        tokio::time::sleep(Duration::from_millis(500)).await;
        for handle in &handles {
            handle.abort();
        }

        assert!(probe.starts() >= 1, "handler should run at least once");
        assert_eq!(probe.max_in_flight(), 1, "handler must not run concurrently when reentry is disabled");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_allow_reentry_runs_handler_concurrently() {
        let probe = ConcurrencyProbe::new();

        let mut scheduler = TaskScheduler::new();
        scheduler.register(config("reentry", true), probe.handler(Duration::from_millis(200)));

        let handles = scheduler.start();
        assert_eq!(handles.len(), 1);

        // 间隔 20ms，200ms 的 handler 在 400ms 内必然发生并发
        tokio::time::sleep(Duration::from_millis(400)).await;
        for handle in &handles {
            handle.abort();
        }

        assert!(
            probe.max_in_flight() >= 2,
            "handler should run concurrently when reentry is allowed"
        );
    }

    #[tokio::test]
    async fn test_start_returns_one_handle_per_registered_task() {
        let mut scheduler = TaskScheduler::new();
        for index in 0..3 {
            scheduler.register(
                TaskConfig {
                    name: format!("task-{index}"),
                    interval: Duration::from_millis(20),
                    allow_reentry: true,
                },
                || async {},
            );
        }

        let handles = scheduler.start();
        assert_eq!(handles.len(), 3);

        for handle in &handles {
            handle.abort();
        }
    }
}
