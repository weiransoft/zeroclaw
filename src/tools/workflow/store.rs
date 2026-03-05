use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use super::template::WorkflowTemplate;

// 工作流模板存储
pub struct WorkflowTemplateStore {
    templates: Mutex<HashMap<String, WorkflowTemplate>>,
    base_dir: PathBuf,
}

impl WorkflowTemplateStore {
    // 创建新的模板存储
    pub fn new(base_dir: PathBuf) -> Self {
        let store = Self {
            templates: Mutex::new(HashMap::new()),
            base_dir,
        };
        store.ensure_directory();
        store.load_templates();
        store
    }
    
    // 确保目录存在
    fn ensure_directory(&self) {
        if !self.base_dir.exists() {
            if let Err(e) = fs::create_dir_all(&self.base_dir) {
                tracing::error!("Failed to create workflow template directory: {:?}", e);
            }
        }
        
        // 检查目录权限
        if let Ok(metadata) = self.base_dir.metadata() {
            if !metadata.is_dir() {
                tracing::error!("Workflow template directory path exists but is not a directory: {:?}", self.base_dir);
            }
        } else {
            tracing::error!("Failed to get workflow template directory metadata: {:?}", self.base_dir);
        }
    }
    
    // 加载模板
    fn load_templates(&self) {
        let mut templates = self.templates.lock().unwrap();
        self.ensure_directory();
        
        match fs::read_dir(&self.base_dir) {
            Ok(entries) => {
                for entry in entries {
                    if let Ok(entry) = entry {
                        if entry.path().extension().unwrap_or_default() == "json" {
                            if let Ok(content) = fs::read_to_string(entry.path()) {
                                if let Ok(template) = serde_json::from_str::<WorkflowTemplate>(&content) {
                                    templates.insert(template.id.clone(), template);
                                } else {
                                    tracing::warn!("Failed to parse workflow template file: {:?}", entry.path());
                                }
                            } else {
                                tracing::warn!("Failed to read workflow template file: {:?}", entry.path());
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to read workflow template directory: {:?}", e);
            }
        }
    }
    
    // 保存模板
    pub fn save_template(&self, template: &WorkflowTemplate) {
        self.ensure_directory();
        
        let file_path = self.base_dir.join(format!("{}.json", template.id));
        match File::create(&file_path) {
            Ok(mut file) => {
                if let Ok(json_str) = serde_json::to_string_pretty(template) {
                    if let Err(e) = write!(file, "{}", json_str) {
                        tracing::error!("Failed to write workflow template file: {:?}", e);
                    }
                } else {
                    tracing::error!("Failed to serialize workflow template: {:?}", template.id);
                }
            }
            Err(e) => {
                tracing::error!("Failed to create workflow template file: {:?}", e);
            }
        }
        
        let mut templates = self.templates.lock().unwrap();
        templates.insert(template.id.clone(), template.clone());
    }
    
    // 获取模板
    pub fn get_template(&self, template_id: &str) -> Option<WorkflowTemplate> {
        let templates = self.templates.lock().unwrap();
        templates.get(template_id).cloned()
    }
    
    // 获取所有模板
    pub fn get_all_templates(&self) -> Vec<WorkflowTemplate> {
        let templates = self.templates.lock().unwrap();
        templates.values().cloned().collect()
    }
    
    // 删除模板
    pub fn delete_template(&self, template_id: &str) -> bool {
        let mut templates = self.templates.lock().unwrap();
        if templates.contains_key(template_id) {
            templates.remove(template_id);
            
            let file_path = self.base_dir.join(format!("{}.json", template_id));
            if file_path.exists() {
                if let Err(e) = fs::remove_file(file_path) {
                    tracing::error!("Failed to delete workflow template file: {:?}", e);
                    return false;
                }
            }
            return true;
        }
        false
    }
    
    // 更新模板
    pub fn update_template(&self, template: &WorkflowTemplate) -> bool {
        let templates = self.templates.lock().unwrap();
        if templates.contains_key(&template.id) {
            self.save_template(template);
            return true;
        }
        false
    }
    
    // 搜索模板
    pub fn search_templates(&self, query: &str) -> Vec<WorkflowTemplate> {
        let templates = self.templates.lock().unwrap();
        let query_lower = query.to_lowercase();
        
        templates.values()
            .filter(|t| {
                t.name.to_lowercase().contains(&query_lower) ||
                t.description.to_lowercase().contains(&query_lower) ||
                t.categories.iter().any(|c| c.to_lowercase().contains(&query_lower)) ||
                t.applicable_scenarios.iter().any(|s| s.to_lowercase().contains(&query_lower))
            })
            .cloned()
            .collect()
    }
    
    // 根据类别获取模板
    pub fn get_templates_by_category(&self, category: &str) -> Vec<WorkflowTemplate> {
        let templates = self.templates.lock().unwrap();
        let category_lower = category.to_lowercase();
        
        templates.values()
            .filter(|t| {
                t.categories.iter().any(|c| c.to_lowercase() == category_lower)
            })
            .cloned()
            .collect()
    }
}

// 工作流模板存储测试
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_template_store() {
        let temp_dir = TempDir::new().unwrap();
        let store = WorkflowTemplateStore::new(temp_dir.path().join("templates"));
        
        // 创建测试模板
        let template = WorkflowTemplate::new(
            "Test Template".to_string(),
            "Test description".to_string(),
            "test_user".to_string(),
        );
        
        // 保存模板
        store.save_template(&template);
        
        // 获取模板
        let retrieved = store.get_template(&template.id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Template");
        
        // 获取所有模板
        let all_templates = store.get_all_templates();
        assert_eq!(all_templates.len(), 1);
        
        // 搜索模板
        let search_results = store.search_templates("test");
        assert_eq!(search_results.len(), 1);
        
        // 删除模板
        let deleted = store.delete_template(&template.id);
        assert!(deleted);
        
        // 验证模板已删除
        let retrieved_after_delete = store.get_template(&template.id);
        assert!(retrieved_after_delete.is_none());
    }
}
