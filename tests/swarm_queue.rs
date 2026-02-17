use futures_util::FutureExt;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use uuid::Uuid;
use zeroclaw::swarm::queue::LaneQueue;

#[tokio::test]
async fn enqueue_runs_jobs() {
    let q = LaneQueue::new(vec![("subagent".to_string(), 2)]);
    let id = Uuid::new_v4();
    let rx = q
        .enqueue(
            "subagent",
            id,
            async move { Ok::<_, anyhow::Error>("ok".to_string()) }.boxed(),
        )
        .await
        .unwrap();
    let out = rx.await.unwrap().unwrap();
    assert_eq!(out, "ok");
}

#[tokio::test]
async fn cancel_pending_cancels_before_start() {
    let q = LaneQueue::new(vec![("subagent".to_string(), 1)]);
    let counter = Arc::new(AtomicU32::new(0));

    let id1 = Uuid::new_v4();
    let rx1 = q
        .enqueue(
            "subagent",
            id1,
            {
                let c = counter.clone();
                async move {
                    sleep(Duration::from_millis(150)).await;
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok("one".to_string())
                }
                .boxed()
            },
        )
        .await
        .unwrap();

    let id2 = Uuid::new_v4();
    let rx2 = q
        .enqueue(
            "subagent",
            id2,
            {
                let c = counter.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok("two".to_string())
                }
                .boxed()
            },
        )
        .await
        .unwrap();

    assert!(q.cancel_pending(id2).await);

    assert_eq!(rx1.await.unwrap().unwrap(), "one");
    let err = rx2.await.unwrap().unwrap_err().to_string();
    assert!(err.contains("cancelled"));
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn abort_running_aborts_task() {
    let q = LaneQueue::new(vec![("subagent".to_string(), 1)]);
    let id = Uuid::new_v4();
    let rx = q
        .enqueue(
            "subagent",
            id,
            async move {
                sleep(Duration::from_secs(30)).await;
                Ok::<_, anyhow::Error>("never".to_string())
            }
            .boxed(),
        )
        .await
        .unwrap();

    sleep(Duration::from_millis(50)).await;
    assert!(q.abort_running(id).await);

    let err = rx.await.unwrap().unwrap_err().to_string();
    assert!(err.contains("aborted"));
}

