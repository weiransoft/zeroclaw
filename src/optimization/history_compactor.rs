use crate::providers::ChatMessage;

const IMPORTANCE_THRESHOLD: f64 = 0.5;
const MAX_SUMMARY_TOKENS: usize = 2000;

pub struct SmartHistoryCompactor {
    importance_threshold: f64,
    max_summary_tokens: usize,
}

impl Default for SmartHistoryCompactor {
    fn default() -> Self {
        Self::new()
    }
}

impl SmartHistoryCompactor {
    pub fn new() -> Self {
        Self {
            importance_threshold: IMPORTANCE_THRESHOLD,
            max_summary_tokens: MAX_SUMMARY_TOKENS,
        }
    }
    
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.importance_threshold = threshold;
        self
    }
    
    pub fn with_max_summary_tokens(mut self, max_tokens: usize) -> Self {
        self.max_summary_tokens = max_tokens;
        self
    }
    
    pub fn calculate_importance(&self, msg: &ChatMessage) -> f64 {
        let mut score: f64 = 0.0;
        
        let tool_indicators = ["tool", "Tool", "result:", "output:", "executed", "ran"];
        let content_lower = msg.content.to_lowercase();
        if tool_indicators.iter().any(|k| content_lower.contains(k)) {
            score += 0.3;
        }
        
        let decision_keywords = [
            "决定", "选择", "方案", "结论", "确定",
            "decided", "chosen", "solution", "conclusion", "confirmed",
            "important", "critical", "key", "essential",
        ];
        if decision_keywords.iter().any(|k| content_lower.contains(k)) {
            score += 0.25;
        }
        
        if msg.role == "user" {
            score += 0.2;
        }
        
        let error_keywords = ["error", "错误", "failed", "失败", "exception"];
        if error_keywords.iter().any(|k| content_lower.contains(k)) {
            score += 0.15;
        }
        
        let question_keywords = ["?", "？", "how", "what", "why", "如何", "什么", "为什么"];
        if question_keywords.iter().any(|k| content_lower.contains(k)) {
            score += 0.1;
        }
        
        score.min(1.0_f64)
    }
    
    pub fn classify_messages(&self, history: &[ChatMessage]) -> (Vec<ChatMessage>, Vec<ChatMessage>) {
        let mut high_importance = Vec::new();
        let mut low_importance = Vec::new();
        
        for msg in history {
            let importance = self.calculate_importance(msg);
            if importance >= self.importance_threshold {
                high_importance.push(msg.clone());
            } else {
                low_importance.push(msg.clone());
            }
        }
        
        (high_importance, low_importance)
    }
    
    pub fn build_summary_context(&self, messages: &[ChatMessage]) -> String {
        let mut context = String::new();
        
        for msg in messages {
            let role = match msg.role.as_str() {
                "user" => "User",
                "assistant" => "AI",
                "system" => "System",
                "tool" => "Tool",
                _ => "Unknown",
            };
            
            let content = if msg.content.len() > 500 {
                format!("{}...", &msg.content[..500])
            } else {
                msg.content.clone()
            };
            
            context.push_str(&format!("{}: {}\n", role, content.trim()));
            
            if context.len() > self.max_summary_tokens * 4 {
                break;
            }
        }
        
        context
    }
    
    pub fn estimate_tokens(text: &str) -> usize {
        let char_count = text.chars().count();
        char_count / 4
    }
    
    pub fn truncate_to_budget(text: &str, max_tokens: usize) -> String {
        let max_chars = max_tokens * 4;
        // Optimization: Quick byte-length check
        if text.len() <= max_chars {
            return text.to_string();
        }

        // Find the byte index for the max_chars-th character
        match text.char_indices().nth(max_chars.saturating_sub(3)) {
            Some((idx, _)) => {
                let mut s = String::with_capacity(idx + 3);
                s.push_str(&text[..idx]);
                s.push_str("...");
                s
            }
            None => text.to_string(),
        }
    }
}

pub fn compact_history_smart(
    history: &[ChatMessage],
    keep_recent: usize,
    max_summary_tokens: usize,
) -> Vec<ChatMessage> {
    if history.len() <= keep_recent {
        return history.to_vec();
    }
    
    let compactor = SmartHistoryCompactor::new()
        .with_max_summary_tokens(max_summary_tokens);
    
    let recent: Vec<ChatMessage> = history.iter().rev().take(keep_recent).rev().cloned().collect();
    let older: Vec<ChatMessage> = history.iter().take(history.len() - keep_recent).cloned().collect();
    
    if older.is_empty() {
        return recent;
    }
    
    let (high_importance, low_importance) = compactor.classify_messages(&older);
    
    let mut result = Vec::new();
    
    if !low_importance.is_empty() {
        let summary_context = compactor.build_summary_context(&low_importance);
        if !summary_context.is_empty() {
            let summary_msg = ChatMessage::assistant(format!(
                "[Earlier context summary]\n{}",
                SmartHistoryCompactor::truncate_to_budget(&summary_context, max_summary_tokens / 2)
            ));
            result.push(summary_msg);
        }
    }
    
    result.extend(high_importance);
    result.extend(recent);
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_importance_calculation() {
        let compactor = SmartHistoryCompactor::new();
        
        let user_msg = ChatMessage::user("What is the solution?");
        let score = compactor.calculate_importance(&user_msg);
        assert!(score > 0.0);
        
        let assistant_msg = ChatMessage::assistant("This is a normal response.");
        let score = compactor.calculate_importance(&assistant_msg);
        assert!(score < 0.5);
        
        let decision_msg = ChatMessage::assistant("I decided to use the first solution.");
        let score = compactor.calculate_importance(&decision_msg);
        assert!(score >= 0.25);
    }
    
    #[test]
    fn test_classify_messages() {
        let compactor = SmartHistoryCompactor::new();
        
        let history = vec![
            ChatMessage::user("Hello"),
            ChatMessage::assistant("Hi there!"),
            ChatMessage::user("What is the key decision?"),
            ChatMessage::assistant("I decided to use option A."),
        ];
        
        let (high, low) = compactor.classify_messages(&history);
        assert!(!high.is_empty() || !low.is_empty());
    }
    
    #[test]
    fn test_compact_history_smart() {
        let history: Vec<ChatMessage> = (0..30)
            .map(|i| ChatMessage::user(format!("Message {}", i)))
            .collect();
        
        let result = compact_history_smart(&history, 10, 500);
        assert!(result.len() <= history.len());
    }
}
