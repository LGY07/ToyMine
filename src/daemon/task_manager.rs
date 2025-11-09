use std::thread::sleep;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::{
    sync::{Notify, mpsc},
    task::JoinHandle,
};

/// 每个任务的控制结构
pub struct TaskHandle<In, Out> {
    pub to_task_tx: mpsc::Sender<In>,
    pub from_task_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<Out>>>,
    pub stop: Arc<Notify>,
    pub handle: JoinHandle<()>,
}

/// 泛型任务管理器
pub struct TaskManager<In, Out> {
    tasks: Arc<Mutex<HashMap<usize, TaskHandle<In, Out>>>>,
}

impl<In: Send + 'static, Out: Send + 'static> TaskManager<In, Out> {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 创建并运行任务
    pub fn spawn_task<F, Fut>(&self, id: usize, func: F)
    where
        F: FnOnce(mpsc::Receiver<In>, mpsc::Sender<Out>, Arc<Notify>) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let (to_task_tx, to_task_rx) = mpsc::channel::<In>(64);
        let (from_task_tx, from_task_rx) = mpsc::channel::<Out>(64);
        let stop = Arc::new(Notify::new());
        let stop_clone = stop.clone();

        let handle = tokio::spawn(async move {
            func(to_task_rx, from_task_tx, stop_clone).await;
        });

        let handle = TaskHandle {
            to_task_tx,
            from_task_rx: Arc::new(tokio::sync::Mutex::new(from_task_rx)),
            stop,
            handle,
        };

        self.tasks.lock().unwrap().insert(id, handle);
    }

    /// 获取任务的发送端（外部 -> 任务）
    pub fn get_sender(&self, id: usize) -> Option<mpsc::Sender<In>> {
        self.tasks
            .lock()
            .unwrap()
            .get(&id)
            .map(|t| t.to_task_tx.clone())
    }

    /// 获取任务的接收端（任务 -> 外部）
    pub fn get_receiver(&self, id: usize) -> Option<Arc<tokio::sync::Mutex<mpsc::Receiver<Out>>>> {
        self.tasks
            .lock()
            .unwrap()
            .get(&id)
            .map(|t| t.from_task_rx.clone())
    }

    /// 停止指定任务
    pub fn stop_task(&self, id: usize) {
        if let Some(t) = self.tasks.lock().unwrap().get(&id) {
            // 人被逼急了什么都做得出来
            for _ in 0..3 {
                sleep(std::time::Duration::from_millis(100));
                t.stop.notify_waiters();
            }
        }
    }

    /// 停止所有任务
    pub fn stop_all(&self) {
        for (_, t) in self.tasks.lock().unwrap().iter() {
            t.stop.notify_one();
        }
    }

    /// 查询任务是否存在
    pub fn exists(&self, id: usize) -> bool {
        self.tasks.lock().unwrap().contains_key(&id)
    }
}
