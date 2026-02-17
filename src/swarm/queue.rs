use anyhow::Result;
use futures_util::future::{AbortHandle, Abortable, Aborted, BoxFuture};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, Notify, Semaphore};
use uuid::Uuid;

pub struct LaneQueue {
    lanes: HashMap<String, Arc<LaneState>>,
    running: Arc<Mutex<HashMap<Uuid, AbortHandle>>>,
}

struct LaneState {
    semaphore: Arc<Semaphore>,
    queue: Mutex<VecDeque<Job>>,
    notify: Notify,
    started: AtomicBool,
}

struct Job {
    id: Uuid,
    fut: BoxFuture<'static, Result<String>>,
    tx: tokio::sync::oneshot::Sender<Result<String>>,
}

impl LaneQueue {
    pub fn new(lanes: Vec<(String, usize)>) -> Arc<Self> {
        let running = Arc::new(Mutex::new(HashMap::new()));
        let mut lane_map = HashMap::new();
        for (name, max) in lanes {
            lane_map.insert(
                name,
                Arc::new(LaneState {
                    semaphore: Arc::new(Semaphore::new(max.max(1))),
                    queue: Mutex::new(VecDeque::new()),
                    notify: Notify::new(),
                    started: AtomicBool::new(false),
                }),
            );
        }

        Arc::new(Self {
            lanes: lane_map,
            running,
        })
    }

    pub async fn enqueue(
        &self,
        lane: &str,
        id: Uuid,
        fut: BoxFuture<'static, Result<String>>,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<String>>> {
        let state = self
            .lanes
            .get(lane)
            .ok_or_else(|| anyhow::anyhow!("unknown lane '{lane}'"))?
            .clone();

        if !state.started.swap(true, Ordering::SeqCst) {
            let lane = lane.to_string();
            let state2 = state.clone();
            let running = self.running.clone();
            tokio::spawn(async move {
                loop {
                    let job = {
                        let mut guard = state2.queue.lock().await;
                        guard.pop_front()
                    };

                    let Some(job) = job else {
                        state2.notify.notified().await;
                        continue;
                    };

                    let permit = match state2.semaphore.clone().acquire_owned().await {
                        Ok(p) => p,
                        Err(_) => {
                            let _ = job.tx.send(Err(anyhow::anyhow!("lane '{lane}' closed")));
                            continue;
                        }
                    };

                    let (abort_handle, abort_reg) = AbortHandle::new_pair();
                    running.lock().await.insert(job.id, abort_handle.clone());

                    let running2 = running.clone();
                    tokio::spawn(async move {
                        let result = Abortable::new(job.fut, abort_reg).await;
                        let result = match result {
                            Ok(inner) => inner,
                            Err(Aborted) => Err(anyhow::anyhow!("aborted")),
                        };
                        let _ = job.tx.send(result);
                        running2.lock().await.remove(&job.id);
                        drop(permit);
                    });
                }
            });
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut guard = state.queue.lock().await;
            guard.push_back(Job { id, fut, tx });
        }
        state.notify.notify_one();
        Ok(rx)
    }

    pub async fn cancel_pending(&self, id: Uuid) -> bool {
        for state in self.lanes.values() {
            let mut guard = state.queue.lock().await;
            if let Some(pos) = guard.iter().position(|j| j.id == id) {
                if let Some(job) = guard.remove(pos) {
                    let _ = job.tx.send(Err(anyhow::anyhow!("cancelled")));
                }
                return true;
            }
        }
        false
    }

    pub async fn abort_running(&self, id: Uuid) -> bool {
        let handle = { self.running.lock().await.get(&id).cloned() };
        if let Some(h) = handle {
            h.abort();
            return true;
        }
        false
    }
}
