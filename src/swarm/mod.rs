use crate::config::{Config, DelegateAgentConfig};
use crate::security::SecurityPolicy;
use crate::swarm::queue::LaneQueue;
use crate::swarm::store::{SwarmChatMessage, SwarmEvent, SwarmSqliteStore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, watch};
use tokio::task::JoinHandle;
use tokio::time::Duration;
use uuid::Uuid;

pub mod queue;
pub mod store;
pub mod chat;
pub mod consensus;
pub mod dependency;

pub use chat::{ChatMessage, ChatMessageType, ConsensusState, SwarmChatManager};
pub use consensus::{ConsensusManager, ConsensusProposal, TaskConsensus, ConsensusStatus};
pub use dependency::{
    TaskDependencyManager, TaskDependency, DependencyGraph, TaskNode, DependencyEdge,
    DependencyAnalysis, TaskCoordinationRequest, TaskCoordinationResponse,
    CoordinationType, CoordinationResponseType, DependencyType, TaskDependencyStatus
};

#[derive(Debug, Clone, Copy)]
pub struct SwarmContext {
    pub depth: u32,
    pub allow_spawn: bool,
}

impl SwarmContext {
    pub fn root() -> Self {
        Self {
            depth: 0,
            allow_spawn: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Terminated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentRun {
    pub run_id: Uuid,
    pub parent_run_id: Option<Uuid>,
    pub agent_name: String,
    pub label: Option<String>,
    pub task: String,
    pub orchestrator: bool,
    pub status: RunStatus,
    pub depth: u32,
    pub started_at_unix: u64,
    pub ended_at_unix: Option<u64>,
    pub output: Option<String>,
    pub error: Option<String>,
    pub children: Vec<Uuid>,
    pub cleanup: bool,
}

impl SubagentRun {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            RunStatus::Completed
                | RunStatus::Failed
                | RunStatus::Cancelled
                | RunStatus::Terminated
        )
    }
}

struct RunEntry {
    run: SubagentRun,
    tx: watch::Sender<SubagentRun>,
    heartbeat: Option<JoinHandle<()>>,
}

pub struct SwarmManager {
    workspace_dir: PathBuf,
    subagent_max_concurrent: usize,
    queue: Arc<LaneQueue>,
    runs: Mutex<HashMap<Uuid, RunEntry>>,
    init_lock: Mutex<()>,
    initialized: AtomicBool,
    store: SwarmSqliteStore,
    instance_id: String,
}

impl SwarmManager {
    pub fn new(workspace_dir: PathBuf, subagent_max_concurrent: usize) -> Arc<Self> {
        let queue = LaneQueue::new(vec![("subagent".to_string(), subagent_max_concurrent)]);
        let store = SwarmSqliteStore::new(&workspace_dir);
        let instance_id = Uuid::new_v4().to_string();
        let mgr = Arc::new(Self {
            workspace_dir,
            subagent_max_concurrent,
            queue,
            runs: Mutex::new(HashMap::new()),
            init_lock: Mutex::new(()),
            initialized: AtomicBool::new(false),
            store,
            instance_id,
        });
        mgr
    }

    async fn ensure_initialized(&self) -> anyhow::Result<()> {
        if self.initialized.load(Ordering::Acquire) {
            return Ok(());
        }
        let _guard = self.init_lock.lock().await;
        if self.initialized.load(Ordering::Acquire) {
            return Ok(());
        }
        let store = self.store.clone();
        let json_path = self.workspace_dir.join(".zeroclaw").join("subagents.json");
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let now = now_unix();
            let _ = store.sweep_stale_inflight(30, now);
            if json_path.exists() && store.count_runs().unwrap_or(0) == 0 {
                let data = std::fs::read_to_string(&json_path)?;
                let runs: Vec<SubagentRun> = serde_json::from_str(&data)?;
                for r in runs {
                    let _ = store.upsert_run(&r, "migrated", None);
                }
                let _ = std::fs::rename(&json_path, json_path.with_extension("json.bak"));
            }
            Ok(())
        })
        .await??;
        self.initialized.store(true, Ordering::Release);
        Ok(())
    }

    async fn mark_running(self: &Arc<Self>, run_id: Uuid) {
        let (rx, run) = {
            let mut guard = self.runs.lock().await;
            let Some(entry) = guard.get_mut(&run_id) else {
                return;
            };
            if !matches!(entry.run.status, RunStatus::Pending) {
                return;
            }
            entry.run.status = RunStatus::Running;
            let _ = entry.tx.send(entry.run.clone());
            (entry.tx.subscribe(), entry.run.clone())
        };

        let store = self.store.clone();
        let owner = self.instance_id.clone();
        let ts = now_unix();
        let _ = tokio::task::spawn_blocking(move || store.upsert_run(&run, &owner, Some(ts))).await;
        let store = self.store.clone();
        let _ = tokio::task::spawn_blocking(move || {
            store.append_event(ts, Some(run_id), "running", &serde_json::json!({}))
        })
        .await;

        let store = self.store.clone();
        let handle = tokio::spawn(async move {
            let mut rx = rx;
            loop {
                if rx.borrow().is_terminal() {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
                if rx.borrow().is_terminal() {
                    break;
                }
                let ts = now_unix();
                let store2 = store.clone();
                let _ = tokio::task::spawn_blocking(move || store2.update_heartbeat(run_id, ts)).await;
            }
        });

        let mut guard = self.runs.lock().await;
        if let Some(entry) = guard.get_mut(&run_id) {
            entry.heartbeat = Some(handle);
        }
    }

    pub async fn spawn(
        self: &Arc<Self>,
        security: &Arc<SecurityPolicy>,
        parent_config: Arc<Config>,
        ctx: SwarmContext,
        agent_name: &str,
        task: &str,
        label: Option<String>,
        orchestrator: bool,
        parent_run_id: Option<Uuid>,
        cleanup: bool,
    ) -> anyhow::Result<Uuid> {
        self.ensure_initialized().await?;
        if !ctx.allow_spawn {
            anyhow::bail!("spawn is not allowed in this swarm context");
        }
        if !security.can_act() {
            anyhow::bail!("Security policy: read-only mode, cannot spawn");
        }
        if !security.record_action() {
            anyhow::bail!("Security policy: action budget exhausted");
        }

        let agent_cfg = parent_config
            .agents
            .get(agent_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown agent '{agent_name}'"))?
            .clone();

        if ctx.depth >= agent_cfg.max_depth {
            anyhow::bail!(
                "Swarm depth limit reached ({}/{})",
                ctx.depth,
                agent_cfg.max_depth
            );
        }

        let run_id = Uuid::new_v4();
        let started = now_unix();
        let run = SubagentRun {
            run_id,
            parent_run_id,
            agent_name: agent_name.to_string(),
            label,
            task: task.to_string(),
            orchestrator,
            status: RunStatus::Pending,
            depth: ctx.depth + 1,
            started_at_unix: started,
            ended_at_unix: None,
            output: None,
            error: None,
            children: Vec::new(),
            cleanup,
        };

        let (tx, _rx) = watch::channel(run.clone());
        let mut parent_children: Option<Vec<Uuid>> = None;
        {
            let mut guard = self.runs.lock().await;
            guard.insert(
                run_id,
                RunEntry {
                    run: run.clone(),
                    tx,
                    heartbeat: None,
                },
            );
            if let Some(parent) = parent_run_id {
                if let Some(parent_entry) = guard.get_mut(&parent) {
                    parent_entry.run.children.push(run_id);
                    let _ = parent_entry.tx.send(parent_entry.run.clone());
                    parent_children = Some(parent_entry.run.children.clone());
                }
            }
        }
        let store = self.store.clone();
        let owner = self.instance_id.clone();
        let run_for_store = run.clone();
        tokio::task::spawn_blocking(move || store.upsert_run(&run_for_store, &owner, Some(started)))
            .await??;
        let store = self.store.clone();
        let agent_name_for_event = agent_name.to_string();
        let _ = tokio::task::spawn_blocking(move || {
            store.append_event(started, Some(run_id), "spawn", &serde_json::json!({ "agent": agent_name_for_event }))
        })
        .await;

        if let Some(parent) = parent_run_id {
            let store = self.store.clone();
            if let Some(children) = parent_children {
                let _ = tokio::task::spawn_blocking(move || store.update_children(parent, &children)).await;
            } else {
                let store = self.store.clone();
                let _ = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                    let Some(mut parent_run) = store.get_run(parent)? else {
                        return Ok(());
                    };
                    if !parent_run.children.contains(&run_id) {
                        parent_run.children.push(run_id);
                        store.update_children(parent, &parent_run.children)?;
                    }
                    Ok(())
                })
                .await;
            }
        }

        let mgr = self.clone();
        let mgr2 = self.clone();
        let allow_spawn = orchestrator;
        let ctx2 = SwarmContext {
            depth: ctx.depth + 1,
            allow_spawn,
        };
        let label2 = run.label.clone();
        let task2 = run.task.clone();
        let parent_run_id2 = parent_run_id;
        let child_cfg = build_child_config(parent_config.as_ref(), &agent_cfg);
        let extra = build_subagent_system_prompt(
            &parent_config.workspace_dir,
            run_id,
            parent_run_id2,
            &agent_cfg,
            &task2,
            label2.as_deref(),
            ctx2.depth,
            ctx2.allow_spawn,
        );
        let fut = async move {
            mgr2.mark_running(run_id).await;
            let out = crate::agent::loop_::process_message_with_swarm_context(
                child_cfg,
                &task2,
                Some(&extra),
                ctx2,
            )
            .await?;
            Ok(out)
        };

        let rx = self
            .queue
            .enqueue("subagent", run_id, Box::pin(fut))
            .await?;

        tokio::spawn(async move {
            let result = match rx.await {
                Ok(inner) => inner,
                Err(e) => Err(anyhow::anyhow!("join error: {e}")),
            };
            mgr.finish_run(run_id, result).await;
        });

        Ok(run_id)
    }

    pub async fn list(&self) -> Vec<SubagentRun> {
        if let Err(e) = self.ensure_initialized().await {
            tracing::warn!("Swarm registry init failed: {e}");
            return Vec::new();
        }
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || store.list_runs())
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_default()
    }

    pub async fn get(&self, run_id: Uuid) -> Option<SubagentRun> {
        if let Err(e) = self.ensure_initialized().await {
            tracing::warn!("Swarm registry init failed: {e}");
            return None;
        }
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || store.get_run(run_id))
            .await
            .ok()
            .and_then(|r| r.ok())
            .flatten()
    }

    pub async fn wait(
        &self,
        run_id: Uuid,
        timeout_secs: Option<u64>,
    ) -> anyhow::Result<SubagentRun> {
        self.ensure_initialized().await?;
        let local_rx = {
            let guard = self.runs.lock().await;
            guard.get(&run_id).map(|e| e.tx.subscribe())
        };
        if let Some(mut rx) = local_rx {
            if rx.borrow().is_terminal() {
                return Ok(rx.borrow().clone());
            }
            let wait_fut = async {
                loop {
                    rx.changed().await?;
                    if rx.borrow().is_terminal() {
                        return Ok::<SubagentRun, anyhow::Error>(rx.borrow().clone());
                    }
                }
            };
            return match timeout_secs {
                Some(0) | None => Ok(wait_fut.await?),
                Some(secs) => Ok(tokio::time::timeout(std::time::Duration::from_secs(secs), wait_fut)
                    .await
                    .map_err(|_| anyhow::anyhow!("timeout"))??),
            };
        }

        let deadline = timeout_secs
            .filter(|s| *s > 0)
            .map(|s| tokio::time::Instant::now() + std::time::Duration::from_secs(s));
        loop {
            if let Some(run) = self.get(run_id).await {
                if run.is_terminal() {
                    return Ok(run);
                }
            } else {
                anyhow::bail!("unknown run_id");
            }
            if let Some(dl) = deadline {
                if tokio::time::Instant::now() >= dl {
                    anyhow::bail!("timeout");
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    pub async fn kill(
        self: &Arc<Self>,
        security: &Arc<SecurityPolicy>,
        run_id: Uuid,
    ) -> anyhow::Result<bool> {
        self.ensure_initialized().await?;
        if !security.can_act() {
            anyhow::bail!("Security policy: read-only mode, cannot kill");
        }
        if !security.record_action() {
            anyhow::bail!("Security policy: action budget exhausted");
        }

        let store = self.store.clone();
        let ids: Vec<Uuid> = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<Uuid>> {
            let runs = store.list_runs()?;
            let mut children_by_parent: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
            for r in runs {
                if let Some(p) = r.parent_run_id {
                    children_by_parent.entry(p).or_default().push(r.run_id);
                }
            }
            let mut stack = vec![run_id];
            let mut out = Vec::new();
            while let Some(id) = stack.pop() {
                out.push(id);
                if let Some(ch) = children_by_parent.get(&id) {
                    stack.extend(ch.iter().copied());
                }
            }
            Ok(out)
        })
        .await??;
        let mut killed_any = false;

        for id in ids {
            let cancelled = self.queue.cancel_pending(id).await;
            let aborted = self.queue.abort_running(id).await;

            if cancelled || aborted {
                killed_any = true;
                let status = if cancelled {
                    RunStatus::Cancelled
                } else {
                    RunStatus::Terminated
                };
                self.set_terminal(id, status, None, Some("killed".to_string()))
                    .await;
            } else {
                let store = self.store.clone();
                let ts = now_unix();
                let _ = tokio::task::spawn_blocking(move || {
                    store.append_event(ts, Some(id), "kill_requested", &serde_json::json!({}))
                })
                .await;
            }
        }

        Ok(killed_any)
    }

    pub async fn steer(
        self: &Arc<Self>,
        security: &Arc<SecurityPolicy>,
        parent_config: Arc<Config>,
        ctx: SwarmContext,
        run_id: Uuid,
        message: &str,
    ) -> anyhow::Result<Uuid> {
        self.ensure_initialized().await?;
        let run = self
            .get(run_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("unknown run_id"))?;

        let _ = self.kill(security, run_id).await;
        self.set_terminal(run_id, RunStatus::Terminated, None, Some("steered".to_string()))
            .await;

        self.spawn(
            security,
            parent_config,
            ctx,
            &run.agent_name,
            message,
            run.label.clone(),
            run.orchestrator,
            run.parent_run_id,
            run.cleanup,
        )
        .await
    }

    async fn finish_run(&self, run_id: Uuid, result: anyhow::Result<String>) {
        if let Some(existing) = self.get(run_id).await {
            if matches!(existing.status, RunStatus::Cancelled | RunStatus::Terminated) {
                return;
            }
        }
        match result {
            Ok(output) => {
                self.set_terminal(run_id, RunStatus::Completed, Some(output), None)
                    .await;
            }
            Err(e) => {
                self.set_terminal(
                    run_id,
                    RunStatus::Failed,
                    None,
                    Some(e.to_string()),
                )
                .await;
            }
        }
    }

    async fn set_terminal(
        &self,
        run_id: Uuid,
        status: RunStatus,
        output: Option<String>,
        error: Option<String>,
    ) {
        let status_for_event = status.clone();
        let (to_remove, run, heartbeat) = {
            let mut guard = self.runs.lock().await;
            let Some(entry) = guard.get_mut(&run_id) else {
                return;
            };
            entry.run.status = status;
            entry.run.ended_at_unix = Some(now_unix());
            entry.run.output = output;
            entry.run.error = error;
            let _ = entry.tx.send(entry.run.clone());
            let to_remove = entry.run.cleanup;
            let run = entry.run.clone();
            let heartbeat = entry.heartbeat.take();
            if to_remove {
                guard.remove(&run_id);
            }
            (to_remove, run, heartbeat)
        };
        if let Some(h) = heartbeat {
            h.abort();
        }

        let store = self.store.clone();
        let owner = self.instance_id.clone();
        let _ = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            if to_remove {
                store.delete_run(run_id)?;
            } else {
                store.upsert_run(&run, &owner, None)?;
            }
            Ok(())
        })
        .await;

        let store = self.store.clone();
        let ts = now_unix();
        let kind = match status_for_event {
            RunStatus::Completed => "completed",
            RunStatus::Failed => "failed",
            RunStatus::Cancelled => "cancelled",
            RunStatus::Terminated => "terminated",
            RunStatus::Pending => "pending",
            RunStatus::Running => "running",
        };
        let _ = tokio::task::spawn_blocking(move || {
            store.append_event(ts, Some(run_id), kind, &serde_json::json!({}))
        })
        .await;
    }
}

fn now_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn build_child_config(parent: &Config, agent: &DelegateAgentConfig) -> Config {
    let mut cfg = parent.clone();
    cfg.default_provider = Some(agent.provider.clone());
    cfg.default_model = Some(agent.model.clone());
    if let Some(t) = agent.temperature {
        cfg.default_temperature = t;
    }
    if let Some(k) = agent.api_key.clone() {
        cfg.api_key = Some(k);
    }
    cfg
}

fn build_subagent_system_prompt(
    _workspace_dir: &Path,
    run_id: Uuid,
    parent_run_id: Option<Uuid>,
    agent: &DelegateAgentConfig,
    task: &str,
    label: Option<&str>,
    depth: u32,
    allow_spawn: bool,
) -> String {
    let mut s = String::new();
    if let Some(p) = agent.system_prompt.as_deref() {
        if !p.trim().is_empty() {
            s.push_str(p.trim());
            s.push_str("\n\n");
        }
    }
    s.push_str("You are a ZeroClaw sub-agent.\n");
    if let Some(l) = label {
        s.push_str(&format!("Task label: {l}\n"));
    }
    s.push_str(&format!("Depth: {depth}\n"));
    s.push_str(&format!("Run id: {run_id}\n"));
    if let Some(pid) = parent_run_id {
        s.push_str(&format!("Parent run id: {pid}\n"));
    }
    if allow_spawn {
        s.push_str("You may spawn additional sub-agents if needed.\n");
    } else {
        s.push_str("You must not spawn additional sub-agents.\n");
    }
    s.push_str("Return a structured result suitable for aggregation.\n\n");
    s.push_str("Use swarm_chat to post progress, questions, and consensus decisions.\n");
    s.push_str("Language: Chinese or English.\n\n");
    s.push_str("Task:\n");
    s.push_str(task);
    s
}

static MANAGERS: OnceLock<std::sync::Mutex<HashMap<PathBuf, Arc<SwarmManager>>>> = OnceLock::new();

pub fn manager_for_workspace(
    workspace_dir: &Path,
    subagent_max_concurrent: usize,
) -> anyhow::Result<Arc<SwarmManager>> {
    let map = MANAGERS.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let mut guard = map.lock().map_err(|_| anyhow::anyhow!("lock poisoned"))?;
    if let Some(m) = guard.get(workspace_dir) {
        if m.subagent_max_concurrent != subagent_max_concurrent {
            tracing::warn!(
                workspace = workspace_dir.display().to_string(),
                existing = m.subagent_max_concurrent,
                requested = subagent_max_concurrent,
                "Swarm manager already initialized with different concurrency; keeping existing"
            );
        }
        return Ok(m.clone());
    }
    let mgr = SwarmManager::new(workspace_dir.to_path_buf(), subagent_max_concurrent);
    guard.insert(workspace_dir.to_path_buf(), mgr.clone());
    Ok(mgr)
}
