use std::collections::HashMap;
use crate::tools::Tool;

pub struct DynamicToolLoader {
    essential_tools: Vec<&'static str>,
    tool_keywords: HashMap<&'static str, Vec<&'static str>>,
}

impl Default for DynamicToolLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl DynamicToolLoader {
    pub fn new() -> Self {
        let essential_tools = vec![
            "shell",
            "file_read",
            "file_write",
            "memory_store",
            "memory_recall",
        ];
        
        let mut tool_keywords = HashMap::new();
        
        tool_keywords.insert("shell", vec!["command", "run", "execute", "bash", "script", "terminal"]);
        tool_keywords.insert("file_read", vec!["read", "file", "open", "view", "cat", "load"]);
        tool_keywords.insert("file_write", vec!["write", "save", "create", "file", "modify", "edit"]);
        tool_keywords.insert("memory_store", vec!["remember", "store", "save", "memory", "note"]);
        tool_keywords.insert("memory_recall", vec!["recall", "remember", "memory", "search", "find"]);
        tool_keywords.insert("memory_forget", vec!["forget", "delete", "remove", "memory"]);
        tool_keywords.insert("http_request", vec!["http", "request", "api", "fetch", "url", "web", "get", "post"]);
        tool_keywords.insert("subagents", vec!["spawn", "subagent", "delegate", "parallel", "task"]);
        tool_keywords.insert("group_chat", vec!["group", "chat", "discuss", "collaborate", "team"]);
        tool_keywords.insert("workflow", vec!["workflow", "pipeline", "automate", "sequence"]);
        tool_keywords.insert("hardware_board_info", vec!["hardware", "board", "device", "arduino", "esp32", "gpio"]);
        tool_keywords.insert("hardware_memory_map", vec!["hardware", "memory", "map", "register"]);
        tool_keywords.insert("hardware_memory_read", vec!["hardware", "memory", "read", "register"]);
        
        Self {
            essential_tools,
            tool_keywords,
        }
    }
    
    pub fn select_tools(&self, context: &str, available_tools: &[Box<dyn Tool>]) -> Vec<Box<dyn Tool>> {
        let mut selected: Vec<Box<dyn Tool>> = Vec::new();
        let mut selected_names: Vec<&str> = Vec::new();
        
        for tool in available_tools {
            let name = tool.name();
            
            if self.essential_tools.contains(&name) {
                selected.push(tool.clone_box());
                selected_names.push(name);
                continue;
            }
            
            if let Some(keywords) = self.tool_keywords.get(name) {
                let context_lower = context.to_lowercase();
                if keywords.iter().any(|k| context_lower.contains(k)) {
                    if !selected_names.contains(&name) {
                        selected.push(tool.clone_box());
                        selected_names.push(name);
                    }
                }
            }
        }
        
        selected
    }
    
    pub fn select_tools_with_explicit(
        &self,
        context: &str,
        available_tools: &[Box<dyn Tool>],
        explicit_tools: &[&str],
    ) -> Vec<Box<dyn Tool>> {
        let mut selected = self.select_tools(context, available_tools);
        
        let explicit_to_add: Vec<Box<dyn Tool>> = available_tools
            .iter()
            .filter(|tool| {
                let name = tool.name();
                explicit_tools.contains(&name) && !selected.iter().any(|t| t.name() == name)
            })
            .map(|t| t.clone_box())
            .collect();
        
        selected.extend(explicit_to_add);
        selected
    }
    
    pub fn get_tool_descriptions(tools: &[Box<dyn Tool>]) -> Vec<(&str, &str)> {
        tools.iter().map(|t| (t.name(), t.description())).collect()
    }
    
    pub fn format_compact_tool_list(tools: &[Box<dyn Tool>]) -> String {
        let mut result = String::new();
        for tool in tools {
            let desc = tool.description();
            let compact_desc = desc.split('.').next().unwrap_or(desc).trim();
            result.push_str(&format!("- {}: {}\n", tool.name(), compact_desc));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dynamic_tool_loader_creation() {
        let loader = DynamicToolLoader::new();
        assert!(!loader.essential_tools.is_empty());
        assert!(loader.tool_keywords.contains_key("shell"));
    }
    
    #[test]
    fn test_select_tools_returns_essential() {
        let loader = DynamicToolLoader::new();
        let context = "hello world";
        let tools: Vec<Box<dyn Tool>> = vec![];
        let selected = loader.select_tools(context, &tools);
        assert!(selected.is_empty());
    }
}
