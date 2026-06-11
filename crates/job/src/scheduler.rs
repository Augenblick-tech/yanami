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
