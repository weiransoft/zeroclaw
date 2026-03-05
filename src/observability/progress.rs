use crate::swarm::store::SwarmSqliteStore;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProgressStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEntry {
    pub id: String,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub status: ProgressStatus,
    pub title: String,
    pub description: Option<String>,
    pub progress: f64,
    pub total: Option<f64>,
    pub unit: Option<String>,
    pub started_at: Option<u64>,
    pub updated_at: u64,
    pub completed_at: Option<u64>,
    pub error: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEntry {
    pub id: String,
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub parent_id: Option<String>,
    pub timestamp: u64,
    pub level: String,
    pub message: String,
    pub lang: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportFilter {
    pub run_id: Option<String>,
    pub task_id: Option<String>,
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,
    pub status: Option<ProgressStatus>,
    pub level: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub progress_entries: Vec<ProgressEntry>,
    pub trace_entries: Vec<TraceEntry>,
    pub export_time: u64,
    pub filter: ExportFilter,
}

pub struct ProgressTraceManager {
    store: Arc<SwarmSqliteStore>,
    workspace_dir: PathBuf,
}

impl ProgressTraceManager {
    pub fn new(workspace_dir: &Path) -> Self {
        Self {
            store: Arc::new(SwarmSqliteStore::new(workspace_dir)),
            workspace_dir: workspace_dir.to_path_buf(),
        }
    }

    pub fn create_progress(
        &self,
        run_id: Option<Uuid>,
        task_id: Option<String>,
        title: String,
        description: Option<String>,
        total: Option<f64>,
        unit: Option<String>,
        metadata: serde_json::Value,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = now_unix();
        let entry = ProgressEntry {
            id: id.clone(),
            run_id: run_id.map(|u| u.to_string()),
            task_id,
            status: ProgressStatus::Pending,
            title,
            description,
            progress: 0.0,
            total,
            unit,
            started_at: None,
            updated_at: now,
            completed_at: None,
            error: None,
            metadata,
        };
        self.store_progress(&entry)?;
        Ok(id)
    }

    pub fn start_progress(&self, id: &str) -> Result<()> {
        let now = now_unix();
        self.update_progress_status(id, ProgressStatus::InProgress, Some(now), None)
    }

    pub fn update_progress(
        &self,
        id: &str,
        progress: f64,
        description: Option<String>,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let now = now_unix();
        self.store.update_progress(id, progress, description.as_deref(), metadata.as_ref(), Some(now))
    }

    pub fn complete_progress(&self, id: &str, _result: Option<String>) -> Result<()> {
        let now = now_unix();
        self.update_progress_status(id, ProgressStatus::Completed, None, Some(now))
    }

    pub fn fail_progress(&self, id: &str, error: String) -> Result<()> {
        let now = now_unix();
        self.store.update_progress_error(id, &error, Some(now))?;
        self.update_progress_status(id, ProgressStatus::Failed, None, Some(now))
    }

    pub fn cancel_progress(&self, id: &str) -> Result<()> {
        let now = now_unix();
        self.update_progress_status(id, ProgressStatus::Cancelled, None, Some(now))
    }

    pub fn add_trace(
        &self,
        run_id: Option<Uuid>,
        task_id: Option<String>,
        parent_id: Option<String>,
        level: String,
        message: String,
        lang: String,
        metadata: serde_json::Value,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = now_unix();
        let entry = TraceEntry {
            id: id.clone(),
            run_id: run_id.map(|u| u.to_string()),
            task_id,
            parent_id,
            timestamp: now,
            level,
            message,
            lang,
            metadata,
        };
        self.store_trace(&entry)?;
        Ok(id)
    }

    pub fn get_progress(&self, id: &str) -> Result<Option<ProgressEntry>> {
        self.store.get_progress(id)
    }

    pub fn list_progress(&self, filter: &ExportFilter) -> Result<Vec<ProgressEntry>> {
        self.store.list_progress(filter)
    }

    pub fn get_trace(&self, id: &str) -> Result<Option<TraceEntry>> {
        self.store.get_trace(id)
    }

    pub fn list_traces(&self, filter: &ExportFilter) -> Result<Vec<TraceEntry>> {
        self.store.list_traces(filter)
    }

    pub fn export(&self, filter: &ExportFilter) -> Result<ExportResult> {
        let progress_entries = self.list_progress(filter)?;
        let trace_entries = self.list_traces(filter)?;
        Ok(ExportResult {
            progress_entries,
            trace_entries,
            export_time: now_unix(),
            filter: filter.clone(),
        })
    }

    pub fn export_to_json(&self, filter: &ExportFilter, path: &Path) -> Result<()> {
        let result = self.export(filter)?;
        let json = serde_json::to_string_pretty(&result)
            .context("Failed to serialize export result")?;
        std::fs::write(path, json)
            .context(format!("Failed to write export to {}", path.display()))?;
        Ok(())
    }

    pub fn export_to_csv(&self, filter: &ExportFilter, path: &Path) -> Result<()> {
        let result = self.export(filter)?;
        let mut csv_content = String::new();

        csv_content.push_str("id,run_id,task_id,status,title,description,progress,total,unit,started_at,updated_at,completed_at,error\n");
        for entry in &result.progress_entries {
            csv_content.push_str(&format!(
                "{},{},{},{:?},{},{},{},{},{},{},{},{},{}\n",
                entry.id,
                entry.run_id.as_deref().unwrap_or(""),
                entry.task_id.as_deref().unwrap_or(""),
                entry.status,
                entry.title,
                entry.description.as_deref().unwrap_or(""),
                entry.progress,
                entry.total.unwrap_or(0.0),
                entry.unit.as_deref().unwrap_or(""),
                entry.started_at.unwrap_or(0),
                entry.updated_at,
                entry.completed_at.unwrap_or(0),
                entry.error.as_deref().unwrap_or("")
            ));
        }

        std::fs::write(path, csv_content)
            .context(format!("Failed to write CSV export to {}", path.display()))?;
        Ok(())
    }

    fn store_progress(&self, entry: &ProgressEntry) -> Result<()> {
        self.store.upsert_progress(entry)
    }

    fn update_progress_status(
        &self,
        id: &str,
        status: ProgressStatus,
        started_at: Option<u64>,
        completed_at: Option<u64>,
    ) -> Result<()> {
        self.store.update_progress_status(id, &status, started_at, completed_at)
    }

    fn store_trace(&self, entry: &TraceEntry) -> Result<()> {
        self.store.upsert_trace(entry)
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_and_update_progress() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ProgressTraceManager::new(temp_dir.path());

        let id = manager
            .create_progress(
                None,
                None,
                "Test Task".to_string(),
                Some("Test description".to_string()),
                Some(100.0),
                Some("%".to_string()),
                serde_json::json!({}),
            )
            .unwrap();

        manager.start_progress(&id).unwrap();
        manager.update_progress(&id, 50.0, None, None).unwrap();
        manager.complete_progress(&id, None).unwrap();

        let entry = manager.get_progress(&id).unwrap().unwrap();
        assert_eq!(entry.status, ProgressStatus::Completed);
        assert_eq!(entry.progress, 50.0);
    }

    #[test]
    fn test_trace_logging() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ProgressTraceManager::new(temp_dir.path());

        let id = manager
            .add_trace(
                None,
                None,
                None,
                "info".to_string(),
                "Test message".to_string(),
                "en".to_string(),
                serde_json::json!({}),
            )
            .unwrap();

        let entry = manager.get_trace(&id).unwrap().unwrap();
        assert_eq!(entry.message, "Test message");
        assert_eq!(entry.lang, "en");
    }

    #[test]
    fn test_export_to_json() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ProgressTraceManager::new(temp_dir.path());

        let _ = manager
            .create_progress(
                None,
                None,
                "Export Test".to_string(),
                None,
                None,
                None,
                serde_json::json!({}),
            )
            .unwrap();

        let export_path = temp_dir.path().join("export.json");
        let filter = ExportFilter {
            run_id: None,
            task_id: None,
            start_time: None,
            end_time: None,
            status: None,
            level: None,
            limit: None,
        };

        manager.export_to_json(&filter, &export_path).unwrap();
        assert!(export_path.exists());
    }
}
