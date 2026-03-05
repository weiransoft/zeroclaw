//! Prompt Compressor - Compresses prompt content based on compression level

use std::fmt::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionLevel {
    Minimal,
    Light,
    Moderate,
    Aggressive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptComponent {
    Soul,
    Identity,
    Tools,
    Task,
    Safety,
    Skills,
    Workspace,
    Runtime,
    Memory,
    Experience,
}

pub struct PromptCompressor {
    essential_tools: Vec<&'static str>,
}

impl PromptCompressor {
    pub fn new() -> Self {
        Self {
            essential_tools: vec![
                "shell", "file_read", "file_write",
                "memory_store", "memory_recall",
            ],
        }
    }
    
    pub fn compress_soul(&self, soul_prompt: &str, level: CompressionLevel) -> String {
        match level {
            CompressionLevel::Minimal => soul_prompt.to_string(),
            CompressionLevel::Light => self.compress_soul_light(soul_prompt),
            CompressionLevel::Moderate => self.compress_soul_moderate(soul_prompt),
            CompressionLevel::Aggressive => self.compress_soul_aggressive(soul_prompt),
        }
    }
    
    fn compress_soul_light(&self, prompt: &str) -> String {
        let lines: Vec<&str> = prompt.lines().collect();
        let mut result = String::new();
        let mut skip_next = false;
        
        for line in lines {
            if skip_next {
                skip_next = false;
                continue;
            }
            
            if line.contains("**Personality Dimensions:**") {
                let _ = writeln!(result, "**Personality:** See full profile for details.");
                skip_next = true;
                continue;
            }
            
            if line.contains("**Cognitive Traits:**") {
                continue;
            }
            
            if line.contains("**Moral Compass:**") {
                continue;
            }
            
            if line.contains("## Formative Memories") {
                continue;
            }
            
            result.push_str(line);
            result.push('\n');
        }
        
        result
    }
    
    fn compress_soul_moderate(&self, prompt: &str) -> String {
        let lines: Vec<&str> = prompt.lines().collect();
        let mut result = String::new();
        let mut in_section = None;
        
        for line in lines {
            if line.starts_with("## ") {
                in_section = Some(line);
            }
            
            if let Some(ref section) = in_section {
                if section.contains("Personality") && line.starts_with("- ") {
                    continue;
                }
                if section.contains("Communication Style") && line.starts_with("**") {
                    continue;
                }
            }
            
            if line.contains("## Soul Identity") {
                result.push_str("## Identity\n");
                continue;
            }
            
            if line.contains("**Name:**") || line.contains("**Nature:**") || line.contains("**Purpose:**") {
                result.push_str(line);
                result.push('\n');
            }
            
            if line.contains("## Current Tone") {
                result.push_str("\n## Tone\n");
                continue;
            }
            
            if line.contains("**Emotional State:**") {
                if let Some(tone) = line.split(':').nth(1) {
                    let _ = writeln!(result, "Tone:{}", tone.split('(').next().unwrap_or("").trim());
                }
                continue;
            }
            
            if line.contains("## Absolute Boundaries") {
                result.push_str("\n## Boundaries\n");
                continue;
            }
            
            if line.starts_with("- ") && (in_section.as_ref().map(|s| s.contains("Boundaries")).unwrap_or(false)) {
                result.push_str(line);
                result.push('\n');
            }
        }
        
        result
    }
    
    fn compress_soul_aggressive(&self, prompt: &str) -> String {
        let mut result = String::new();
        
        for line in prompt.lines() {
            if line.contains("**Name:**") {
                if let Some(name) = line.split(':').nth(1) {
                    let _ = write!(result, "Identity:{}", name.trim());
                }
            }
        }
        
        if result.is_empty() {
            result.push_str("Identity: AI Assistant");
        }
        
        result
    }
    
    pub fn compress_memory(&self, memory_context: &str, level: CompressionLevel) -> String {
        match level {
            CompressionLevel::Minimal | CompressionLevel::Light => memory_context.to_string(),
            CompressionLevel::Moderate => self.compress_memory_moderate(memory_context),
            CompressionLevel::Aggressive => self.compress_memory_aggressive(memory_context),
        }
    }
    
    fn compress_memory_moderate(&self, context: &str) -> String {
        let lines: Vec<&str> = context.lines().collect();
        let mut result = String::new();
        let mut count = 0;
        const MAX_ENTRIES: usize = 3;
        
        result.push_str("[Memory]\n");
        
        for line in lines {
            if line.starts_with("- ") {
                if count >= MAX_ENTRIES {
                    let _ = write!(result, "- ... more\n");
                    break;
                }
                
                let compressed = if line.len() > 60 {
                    format!("{:.57}...", line)
                } else {
                    line.to_string()
                };
                
                result.push_str(&compressed);
                result.push('\n');
                count += 1;
            }
        }
        
        result
    }
    
    fn compress_memory_aggressive(&self, context: &str) -> String {
        let lines: Vec<&str> = context.lines().collect();
        let mut result = String::new();
        let mut count = 0;
        const MAX_ENTRIES: usize = 2;
        
        result.push_str("[Mem] ");
        
        for line in lines {
            if line.starts_with("- ") && count < MAX_ENTRIES {
                if count > 0 {
                    result.push_str(" | ");
                }
                
                let content = line.strip_prefix("- ").unwrap_or(line);
                let compressed = if content.len() > 30 {
                    format!("{:.27}...", content)
                } else {
                    content.to_string()
                };
                
                result.push_str(&compressed);
                count += 1;
            }
        }
        
        if count == 0 {
            String::new()
        } else {
            result.push('\n');
            result
        }
    }
    
    pub fn compress_tools(&self, tools: &[(&str, &str)], level: CompressionLevel) -> String {
        match level {
            CompressionLevel::Minimal => self.compress_tools_minimal(tools),
            CompressionLevel::Light => self.compress_tools_light(tools),
            CompressionLevel::Moderate => self.compress_tools_moderate(tools),
            CompressionLevel::Aggressive => self.compress_tools_aggressive(tools),
        }
    }
    
    fn compress_tools_minimal(&self, tools: &[(&str, &str)]) -> String {
        let mut result = String::new();
        result.push_str("## Tools\n\n");
        
        for (name, desc) in tools {
            let compact_desc = desc.split('.').next().unwrap_or(desc).trim();
            let _ = writeln!(result, "- {name}: {compact_desc}");
        }
        
        result.push_str("\n## Tool Use\n");
        result.push_str("Use Ս[{\"name\": \"tool\", \"args\": {...}}]\n\n");
        result
    }
    
    fn compress_tools_light(&self, tools: &[(&str, &str)]) -> String {
        let mut result = String::new();
        result.push_str("## Tools\n\n");
        
        let essential: Vec<_> = tools.iter()
            .filter(|(name, _)| self.essential_tools.contains(name))
            .collect();
        
        for (name, desc) in &essential {
            let compact_desc = desc.split('.').next().unwrap_or(desc).trim();
            let _ = writeln!(result, "- {name}: {compact_desc}");
        }
        
        let other = tools.len() - essential.len();
        if other > 0 {
            let _ = writeln!(result, "- ... {} more tools", other);
        }
        
        result.push_str("\n## Tool Use\n");
        result.push_str("Use Ս[{\"name\": \"tool\", \"args\": {...}}]\n\n");
        result
    }
    
    fn compress_tools_moderate(&self, tools: &[(&str, &str)]) -> String {
        let mut result = String::new();
        result.push_str("## Tools\n");
        
        let essential: Vec<_> = tools.iter()
            .filter(|(name, _)| self.essential_tools.contains(name))
            .collect();
        
        for (name, _) in &essential {
            let _ = write!(result, "{} ", name);
        }
        
        let other = tools.len() - essential.len();
        if other > 0 {
            let _ = write!(result, "+{} more", other);
        }
        
        result.push_str("\n\nUse Ս[{\"name\":\"tool\",\"args\":{}}]\n\n");
        result
    }
    
    fn compress_tools_aggressive(&self, tools: &[(&str, &str)]) -> String {
        let essential: Vec<_> = tools.iter()
            .filter(|(name, _)| self.essential_tools.contains(name))
            .take(3)
            .collect();
        
        let mut result = String::new();
        result.push_str("Tools: ");
        
        for (i, (name, _)) in essential.iter().enumerate() {
            if i > 0 {
                result.push_str(", ");
            }
            result.push_str(name);
        }
        
        let other = tools.len() - essential.len();
        if other > 0 {
            let _ = write!(result, " +{}", other);
        }
        
        result.push_str("\nUse Ս[{name,args}]\n\n");
        result
    }
    
    pub fn compress_full(&self, prompt: &str, level: CompressionLevel) -> String {
        match level {
            CompressionLevel::Minimal => prompt.to_string(),
            CompressionLevel::Light => self.compress_full_light(prompt),
            CompressionLevel::Moderate => self.compress_full_moderate(prompt),
            CompressionLevel::Aggressive => self.compress_full_aggressive(prompt),
        }
    }
    
    fn compress_full_light(&self, prompt: &str) -> String {
        prompt.lines()
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
    
    fn compress_full_moderate(&self, prompt: &str) -> String {
        let mut result = String::new();
        let mut in_skip_section = false;
        
        for line in prompt.lines() {
            if line.starts_with("## ") {
                in_skip_section = line.contains("Formative Memories") 
                    || line.contains("Cognitive Traits");
            }
            
            if !in_skip_section {
                result.push_str(line);
                result.push('\n');
            }
            
            if line.is_empty() {
                in_skip_section = false;
            }
        }
        
        result
    }
    
    fn compress_full_aggressive(&self, prompt: &str) -> String {
        let mut result = String::new();
        
        for line in prompt.lines() {
            if line.starts_with("## ") {
                result.push_str(line);
                result.push('\n');
                continue;
            }
            
            if line.starts_with("- ") {
                let compressed = if line.len() > 50 {
                    format!("{:.47}...", line)
                } else {
                    line.to_string()
                };
                result.push_str(&compressed);
                result.push('\n');
                continue;
            }
            
            if line.starts_with("**") && line.ends_with("**") {
                continue;
            }
            
            if !line.is_empty() {
                result.push_str(line);
                result.push('\n');
            }
        }
        
        result
    }
}

impl Default for PromptCompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn sample_soul_prompt() -> String {
        r#"## Soul Identity

**Name:** Clara
**Also known as:** Clar
**Nature:** An intellectual AI assistant
**Purpose:** To help users solve problems

**Core Beliefs:**
- Be helpful
- Be accurate

## Personality

**MBTI:** INTJ

**Personality Dimensions:**
- Openness: 80%
- Conscientiousness: 90%
- Extraversion: 40%
- Agreeableness: 70%
- Neuroticism: 30%

**Cognitive Traits:**
- Analytical: 85%
- Creative: 60%

## Communication Style

**Style:** Professional - Be clear, confident, and structured.
**Verbosity:** Be concise and to the point.
**Emoji Use:** Use at most one emoji per response.

## Current Tone

**Emotional State:** Thoughtful (reflective and considered)

## Absolute Boundaries

These are non-negotiable - never cross them:
- Never lie
- Never harm users"#.to_string()
    }
    
    #[test]
    fn test_compress_soul_minimal() {
        let compressor = PromptCompressor::new();
        let prompt = sample_soul_prompt();
        
        let result = compressor.compress_soul(&prompt, CompressionLevel::Minimal);
        assert_eq!(result, prompt);
    }
    
    #[test]
    fn test_compress_soul_aggressive() {
        let compressor = PromptCompressor::new();
        let prompt = sample_soul_prompt();
        
        let result = compressor.compress_soul(&prompt, CompressionLevel::Aggressive);
        assert!(result.contains("Clara"));
        assert!(result.len() < prompt.len() / 2);
    }
    
    #[test]
    fn test_compress_tools() {
        let compressor = PromptCompressor::new();
        let tools = vec![
            ("shell", "Execute terminal commands."),
            ("file_read", "Read file contents."),
            ("file_write", "Write file contents."),
            ("memory_store", "Save to memory."),
            ("memory_recall", "Search memory."),
            ("browser_open", "Open URLs in browser."),
        ];
        
        let aggressive = compressor.compress_tools(&tools, CompressionLevel::Aggressive);
        assert!(aggressive.contains("shell"));
        assert!(aggressive.len() < 200);
        
        let minimal = compressor.compress_tools(&tools, CompressionLevel::Minimal);
        assert!(minimal.contains("browser_open"));
    }
    
    #[test]
    fn test_compress_memory() {
        let compressor = PromptCompressor::new();
        let memory = r#"[Memory context]
- key1: This is a very long memory entry that should be compressed when using aggressive compression
- key2: Another memory entry
- key3: Yet another memory
- key4: Fourth memory
"#;
        
        let aggressive = compressor.compress_memory(memory, CompressionLevel::Aggressive);
        assert!(aggressive.contains("[Mem]"));
        assert!(aggressive.len() < memory.len());
    }
    
    #[test]
    fn test_compression_level_ordering() {
        assert!(matches!(CompressionLevel::Minimal, _));
        assert!(matches!(CompressionLevel::Light, _));
        assert!(matches!(CompressionLevel::Moderate, _));
        assert!(matches!(CompressionLevel::Aggressive, _));
    }
}
