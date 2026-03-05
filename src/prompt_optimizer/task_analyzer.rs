//! Task Analyzer - Determines task type from user message
//!
//! Analyzes user messages to classify them into task types,
//! which determines the appropriate prompt compression level.

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskType {
    Quick,
    Simple,
    Standard,
    Complex,
    Technical,
    Creative,
    Conversation,
    Orchestrator,
}

impl TaskType {
    pub fn description(&self) -> &'static str {
        match self {
            Self::Quick => "Quick lookup or simple question",
            Self::Simple => "Single tool operation",
            Self::Standard => "Standard task requiring moderate context",
            Self::Complex => "Complex multi-step task",
            Self::Technical => "Technical implementation task",
            Self::Creative => "Creative or content generation",
            Self::Conversation => "Conversational interaction",
            Self::Orchestrator => "Multi-agent orchestration",
        }
    }
    
    pub fn requires_full_context(&self) -> bool {
        matches!(self, Self::Complex | Self::Technical | Self::Orchestrator)
    }
    
    pub fn requires_personality(&self) -> bool {
        matches!(self, Self::Creative | Self::Conversation | Self::Complex)
    }
}

pub struct TaskAnalyzer {
    quick_patterns: Vec<&'static str>,
    simple_patterns: Vec<&'static str>,
    complex_patterns: Vec<&'static str>,
    technical_patterns: Vec<&'static str>,
    creative_patterns: Vec<&'static str>,
    conversation_patterns: Vec<&'static str>,
    orchestrator_patterns: Vec<&'static str>,
    tool_keywords: HashSet<&'static str>,
}

impl TaskAnalyzer {
    pub fn new() -> Self {
        Self {
            quick_patterns: vec![
                "what is", "what's", "how many", "is it", "are there",
                "define", "explain briefly", "quick", "simple question",
                "yes or no", "true or false", "calculate", "convert",
                "what time", "what date", "when is", "who is", "where is",
                "多少", "什么是", "是不是", "有没有", "计算", "转换",
            ],
            simple_patterns: vec![
                "read file", "write file", "list files", "show me",
                "run command", "execute", "check if", "get the",
                "find the", "search for", "look up", "fetch",
                "读取", "写入", "列出", "运行", "执行", "查找", "搜索",
            ],
            complex_patterns: vec![
                "design", "implement", "create a", "build a", "develop",
                "refactor", "optimize", "architect", "integrate",
                "design and implement", "full system", "complete solution",
                "设计", "实现", "创建", "构建", "开发", "重构", "优化", "架构",
            ],
            technical_patterns: vec![
                "debug", "fix bug", "performance", "algorithm", "optimize",
                "database", "api", "backend", "frontend", "infrastructure",
                "deploy", "configure", "security", "authentication",
                "调试", "修复", "性能", "算法", "数据库", "配置", "安全",
            ],
            creative_patterns: vec![
                "write a story", "create content", "generate", "compose",
                "write a poem", "creative", "brainstorm", "ideate",
                "design a ui", "create a design", "artistic",
                "写一个故事", "创作", "生成", "头脑风暴", "设计",
            ],
            conversation_patterns: vec![
                "let's talk", "chat", "discuss", "what do you think",
                "opinion", "feel", "believe", "recommend me",
                "help me decide", "advice", "suggestion",
                "聊聊", "讨论", "觉得", "认为", "建议", "意见",
            ],
            orchestrator_patterns: vec![
                "coordinate", "orchestrate", "multiple agents", "parallel",
                "distribute", "delegate", "subtasks", "workflow",
                "协调", "编排", "并行", "分配", "委托", "工作流",
            ],
            tool_keywords: [
                "file", "read", "write", "shell", "command", "execute",
                "memory", "recall", "store", "search", "find",
                "browser", "open", "screenshot", "image",
                "gpio", "hardware", "arduino",
            ].iter().cloned().collect(),
        }
    }
    
    pub fn analyze(&self, message: &str, tools_available: &[&str]) -> TaskType {
        let msg_lower = message.to_lowercase();
        let words: Vec<&str> = msg_lower.split_whitespace().collect();
        let word_count = words.len();
        
        if self.matches_patterns(&msg_lower, &self.orchestrator_patterns) {
            return TaskType::Orchestrator;
        }
        
        if self.matches_patterns(&msg_lower, &self.complex_patterns) {
            return TaskType::Complex;
        }
        
        if self.matches_patterns(&msg_lower, &self.technical_patterns) {
            return TaskType::Technical;
        }
        
        if self.matches_patterns(&msg_lower, &self.creative_patterns) {
            return TaskType::Creative;
        }
        
        if self.matches_patterns(&msg_lower, &self.conversation_patterns) {
            return TaskType::Conversation;
        }
        
        if word_count <= 5 && !self.requires_tools(&msg_lower, tools_available) {
            return TaskType::Quick;
        }
        
        if self.is_simple_tool_operation(&msg_lower, tools_available) {
            return TaskType::Simple;
        }
        
        if self.requires_tools(&msg_lower, tools_available) {
            return TaskType::Standard;
        }
        
        if word_count <= 10 {
            return TaskType::Quick;
        }
        
        TaskType::Standard
    }
    
    fn matches_patterns(&self, message: &str, patterns: &[&'static str]) -> bool {
        patterns.iter().any(|p| message.contains(p))
    }
    
    fn is_simple_tool_operation(&self, message: &str, tools_available: &[&str]) -> bool {
        let tool_indicators = [
            ("read", "file_read"),
            ("write", "file_write"),
            ("list", "shell"),
            ("show", "file_read"),
            ("run", "shell"),
            ("execute", "shell"),
            ("find", "memory_recall"),
            ("search", "memory_recall"),
            ("recall", "memory_recall"),
        ];
        
        for (indicator, tool) in tool_indicators {
            if message.contains(indicator) && tools_available.contains(&tool) {
                let word_count = message.split_whitespace().count();
                if word_count <= 15 {
                    return true;
                }
            }
        }
        
        false
    }
    
    fn requires_tools(&self, message: &str, _tools_available: &[&str]) -> bool {
        self.tool_keywords.iter().any(|kw| message.contains(kw))
    }
}

impl Default for TaskAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_quick_questions() {
        let analyzer = TaskAnalyzer::new();
        
        assert_eq!(analyzer.analyze("what is 2+2", &[]), TaskType::Quick);
        assert_eq!(analyzer.analyze("define AI", &[]), TaskType::Quick);
        assert_eq!(analyzer.analyze("how many days in a week", &[]), TaskType::Quick);
        assert_eq!(analyzer.analyze("什么是人工智能", &[]), TaskType::Quick);
    }
    
    #[test]
    fn test_simple_operations() {
        let analyzer = TaskAnalyzer::new();
        let tools = vec!["file_read", "shell"];
        
        assert_eq!(analyzer.analyze("read the file config.toml", &tools), TaskType::Simple);
        assert_eq!(analyzer.analyze("list files in directory", &tools), TaskType::Simple);
    }
    
    #[test]
    fn test_complex_tasks() {
        let analyzer = TaskAnalyzer::new();
        
        assert_eq!(analyzer.analyze("design and implement a complete authentication system", &[]), TaskType::Complex);
        assert_eq!(analyzer.analyze("create a full stack web application", &[]), TaskType::Complex);
    }
    
    #[test]
    fn test_technical_tasks() {
        let analyzer = TaskAnalyzer::new();
        
        assert_eq!(analyzer.analyze("fix the api authentication bug", &[]), TaskType::Technical);
        assert_eq!(analyzer.analyze("configure the database connection", &[]), TaskType::Technical);
    }
    
    #[test]
    fn test_creative_tasks() {
        let analyzer = TaskAnalyzer::new();
        
        assert_eq!(analyzer.analyze("write a story about a robot", &[]), TaskType::Creative);
        assert_eq!(analyzer.analyze("create content for my blog", &[]), TaskType::Creative);
    }
    
    #[test]
    fn test_conversation_tasks() {
        let analyzer = TaskAnalyzer::new();
        
        assert_eq!(analyzer.analyze("let's talk about philosophy", &[]), TaskType::Conversation);
        assert_eq!(analyzer.analyze("what do you think about AI", &[]), TaskType::Conversation);
    }
    
    #[test]
    fn test_orchestrator_tasks() {
        let analyzer = TaskAnalyzer::new();
        
        assert_eq!(analyzer.analyze("coordinate multiple agents to complete this task", &[]), TaskType::Orchestrator);
        assert_eq!(analyzer.analyze("orchestrate the workflow", &[]), TaskType::Orchestrator);
    }
    
    #[test]
    fn test_task_type_properties() {
        assert!(TaskType::Complex.requires_full_context());
        assert!(TaskType::Creative.requires_personality());
        assert!(!TaskType::Quick.requires_full_context());
        assert!(!TaskType::Quick.requires_personality());
    }
}
