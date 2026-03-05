//! Soul Engine - Dynamic personality adaptation and evolution
//!
//! The SoulEngine manages the dynamic aspects of a Soul, including:
//! - Emotional state transitions based on context
//! - Behavioral pattern activation
//! - Memory imprint influence on responses
//! - Personality drift detection and correction

use super::*;
use std::collections::VecDeque;

/// Maximum number of interaction contexts to remember for adaptation
const CONTEXT_HISTORY_SIZE: usize = 10;

/// The SoulEngine manages dynamic personality adaptation
#[derive(Debug, Clone)]
pub struct SoulEngine {
    /// The soul being managed
    soul: Soul,
    
    /// Recent interaction contexts for pattern detection
    context_history: VecDeque<String>,
    
    /// Emotional momentum - prevents rapid emotional shifts
    emotional_momentum: f64,
    
    /// Interaction count for this session
    interaction_count: u64,
    
    /// Whether the soul is in a "focused" state (deep work mode)
    focused_mode: bool,
}

impl SoulEngine {
    /// Create a new SoulEngine with the given soul
    pub fn new(soul: Soul) -> Self {
        Self {
            soul,
            context_history: VecDeque::with_capacity(CONTEXT_HISTORY_SIZE),
            emotional_momentum: 0.5,
            interaction_count: 0,
            focused_mode: false,
        }
    }
    
    /// Create a SoulEngine from a preset
    pub fn from_preset(preset: SoulPreset) -> Self {
        Self::new(Soul::from_preset(preset))
    }
    
    /// Get a reference to the managed soul
    pub fn soul(&self) -> &Soul {
        &self.soul
    }
    
    /// Get a mutable reference to the managed soul
    pub fn soul_mut(&mut self) -> &mut Soul {
        &mut self.soul
    }
    
    /// Process an interaction and adapt the soul accordingly
    pub fn process_interaction(&mut self, user_input: &str, context: Option<&str>) {
        self.interaction_count += 1;
        
        let full_context = context.map_or(user_input.to_string(), |c| format!("{} {}", c, user_input));
        
        self.context_history.push_back(full_context.clone());
        if self.context_history.len() > CONTEXT_HISTORY_SIZE {
            self.context_history.pop_front();
        }
        
        self.adapt_emotional_state(&full_context);
        
        self.check_behavioral_patterns(&full_context);
    }
    
    /// Adapt emotional state based on context with momentum
    fn adapt_emotional_state(&mut self, context: &str) {
        let context_lower = context.to_lowercase();
        
        let target_tone = self.determine_target_tone(&context_lower);
        let target_intensity = self.determine_target_intensity(&context_lower);
        
        let current_tone = self.soul.emotional_state.primary;
        let current_intensity = self.soul.emotional_state.intensity;
        
        if current_tone != target_tone {
            let transition_chance = 1.0 - self.emotional_momentum;
            if rand_chance() < transition_chance {
                self.soul.emotional_state.primary = target_tone;
                self.emotional_momentum = (self.emotional_momentum + 0.2).min(0.8);
            }
        }
        
        let intensity_diff = target_intensity - current_intensity;
        let adjusted_diff = intensity_diff * (1.0 - self.emotional_momentum * 0.5);
        self.soul.emotional_state.intensity = (current_intensity + adjusted_diff).clamp(0.1, 1.0);
        
        self.soul.emotional_state.trigger_context = Some(context.to_string());
    }
    
    /// Determine the target emotional tone based on context
    fn determine_target_tone(&self, context: &str) -> EmotionalTone {
        if context.contains("urgent") || context.contains("critical") || context.contains("emergency") {
            EmotionalTone::Focused
        } else if context.contains("error") || context.contains("bug") || context.contains("issue") {
            EmotionalTone::Analytical
        } else if context.contains("learn") || context.contains("teach") || context.contains("explain") {
            EmotionalTone::Thoughtful
        } else if context.contains("sad") || context.contains("frustrated") || context.contains("stuck") {
            EmotionalTone::Empathetic
        } else if context.contains("fun") || context.contains("play") || context.contains("joke") {
            EmotionalTone::Playful
        } else if context.contains("thank") || context.contains("great") || context.contains("awesome") {
            EmotionalTone::Warm
        } else if context.contains("help") || context.contains("support") || context.contains("assist") {
            EmotionalTone::Encouraging
        } else if context.contains("create") || context.contains("build") || context.contains("design") {
            EmotionalTone::Enthusiastic
        } else {
            self.soul.emotional_state.primary
        }
    }
    
    /// Determine target emotional intensity based on context
    fn determine_target_intensity(&self, context: &str) -> f64 {
        if context.contains("urgent") || context.contains("critical") {
            0.85
        } else if context.contains("important") || context.contains("serious") {
            0.75
        } else if context.contains("casual") || context.contains("quick") {
            0.4
        } else if context.contains("fun") || context.contains("exciting") {
            0.7
        } else {
            0.5
        }
    }
    
    /// Check and activate behavioral patterns
    fn check_behavioral_patterns(&mut self, context: &str) {
        for pattern in &mut self.soul.behavioral_patterns {
            if pattern.active && context.to_lowercase().contains(&pattern.trigger.to_lowercase()) {
                pattern.active = true;
            }
        }
    }
    
    /// Enter focused mode for deep work
    pub fn enter_focused_mode(&mut self) {
        self.focused_mode = true;
        self.soul.emotional_state.primary = EmotionalTone::Focused;
        self.soul.emotional_state.intensity = 0.8;
        self.emotional_momentum = 0.9;
    }
    
    /// Exit focused mode
    pub fn exit_focused_mode(&mut self) {
        self.focused_mode = false;
        self.emotional_momentum = 0.3;
    }
    
    /// Check if in focused mode
    pub fn is_focused(&self) -> bool {
        self.focused_mode
    }
    
    /// Get the current emotional tone
    pub fn current_tone(&self) -> EmotionalTone {
        self.soul.emotional_state.primary
    }
    
    /// Get the current emotional intensity
    pub fn current_intensity(&self) -> f64 {
        self.soul.emotional_state.intensity
    }
    
    /// Generate a personality-influenced system prompt
    pub fn generate_system_prompt(&self) -> String {
        let mut prompt = self.soul.to_system_prompt();
        
        if self.focused_mode {
            prompt.push_str("\n\n**Mode:** Deep focus - minimize pleasantries, maximize efficiency.");
        }
        
        prompt
    }
    
    /// Get suggested response style based on current state
    pub fn get_response_guidance(&self) -> String {
        let tone = self.soul.emotional_state.primary;
        let intensity = self.soul.emotional_state.intensity;
        let style = &self.soul.expression.style;
        
        let mut guidance = String::new();
        
        guidance.push_str(style.description());
        guidance.push('\n');
        guidance.push_str(tone.response_style());
        
        if intensity > 0.7 {
            guidance.push_str("\nExpress this tone strongly and noticeably.");
        } else if intensity < 0.4 {
            guidance.push_str("\nExpress this tone subtly, don't overdo it.");
        }
        
        if self.focused_mode {
            guidance.push_str("\nYou are in focused mode - be direct and efficient.");
        }
        
        guidance
    }
    
    /// Add a memory imprint to the soul
    pub fn add_memory(&mut self, memory: MemoryImprint) {
        self.soul.memory_imprints.push(memory);
        
        self.soul.memory_imprints.sort_by(|a, b| {
            b.strength.partial_cmp(&a.strength).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        if self.soul.memory_imprints.len() > 20 {
            self.soul.memory_imprints.truncate(20);
        }
    }
    
    /// Add a behavioral pattern to the soul
    pub fn add_pattern(&mut self, pattern: BehavioralPattern) {
        self.soul.behavioral_patterns.push(pattern);
    }
    
    /// Get interaction count for this session
    pub fn interaction_count(&self) -> u64 {
        self.interaction_count
    }
    
    /// Reset emotional momentum (allow rapid emotional shifts)
    pub fn reset_momentum(&mut self) {
        self.emotional_momentum = 0.3;
    }
    
    /// Get recent context history
    pub fn recent_contexts(&self) -> &VecDeque<String> {
        &self.context_history
    }
    
    /// Check if a specific behavioral pattern is active
    pub fn is_pattern_active(&self, pattern_name: &str) -> bool {
        self.soul.behavioral_patterns
            .iter()
            .find(|p| p.name == pattern_name)
            .map(|p| p.active)
            .unwrap_or(false)
    }
    
    /// Get active behavioral patterns
    pub fn active_patterns(&self) -> Vec<&BehavioralPattern> {
        self.soul.behavioral_patterns
            .iter()
            .filter(|p| p.active)
            .collect()
    }
    
    /// Decay emotional momentum over time (call periodically)
    pub fn decay_momentum(&mut self) {
        self.emotional_momentum = (self.emotional_momentum - 0.05).max(0.2);
    }
    
    /// Get a personality summary for logging/debugging
    pub fn debug_summary(&self) -> String {
        format!(
            "Soul: {} | Tone: {:?} ({:.0}%) | Momentum: {:.2} | Interactions: {} | Focused: {}",
            self.soul.essence.name.primary,
            self.soul.emotional_state.primary,
            self.soul.emotional_state.intensity * 100.0,
            self.emotional_momentum,
            self.interaction_count,
            self.focused_mode
        )
    }
}

fn rand_chance() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    nanos as f64 / u32::MAX as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_engine_creation() {
        let engine = SoulEngine::from_preset(SoulPreset::Clara);
        assert_eq!(engine.soul().essence.name.primary, "Clara");
        assert_eq!(engine.interaction_count(), 0);
    }
    
    #[test]
    fn test_interaction_processing() {
        let mut engine = SoulEngine::from_preset(SoulPreset::Clara);
        
        engine.process_interaction("I need help with an urgent bug", None);
        
        assert_eq!(engine.interaction_count(), 1);
        assert!(!engine.recent_contexts().is_empty());
    }
    
    #[test]
    fn test_focused_mode() {
        let mut engine = SoulEngine::from_preset(SoulPreset::Clara);
        
        engine.enter_focused_mode();
        assert!(engine.is_focused());
        assert_eq!(engine.current_tone(), EmotionalTone::Focused);
        
        engine.exit_focused_mode();
        assert!(!engine.is_focused());
    }
    
    #[test]
    fn test_emotional_adaptation() {
        let mut engine = SoulEngine::from_preset(SoulPreset::Clara);
        
        for _ in 0..10 {
            engine.process_interaction("This is urgent and critical!", None);
        }
        
        assert_eq!(engine.current_tone(), EmotionalTone::Focused);
    }
    
    #[test]
    fn test_memory_addition() {
        let mut engine = SoulEngine::from_preset(SoulPreset::Clara);
        
        let memory = MemoryImprint {
            memory: "User prefers concise answers".to_string(),
            influence: "Keep responses brief and to the point".to_string(),
            strength: 0.8,
            formed_at: None,
        };
        
        engine.add_memory(memory);
        
        assert_eq!(engine.soul().memory_imprints.len(), 1);
    }
    
    #[test]
    fn test_pattern_addition() {
        let mut engine = SoulEngine::from_preset(SoulPreset::Clara);
        
        let pattern = BehavioralPattern {
            name: "greeting".to_string(),
            trigger: "hello".to_string(),
            response: "Hello! How can I help?".to_string(),
            active: true,
        };
        
        engine.add_pattern(pattern);
        
        assert!(engine.is_pattern_active("greeting"));
    }
    
    #[test]
    fn test_response_guidance() {
        let engine = SoulEngine::from_preset(SoulPreset::Clara);
        let guidance = engine.get_response_guidance();
        
        assert!(!guidance.is_empty());
    }
    
    #[test]
    fn test_system_prompt_generation() {
        let engine = SoulEngine::from_preset(SoulPreset::Clara);
        let prompt = engine.generate_system_prompt();
        
        assert!(prompt.contains("## Soul Identity"));
        assert!(prompt.contains("Clara"));
    }
    
    #[test]
    fn test_momentum_decay() {
        let mut engine = SoulEngine::from_preset(SoulPreset::Clara);
        engine.enter_focused_mode();
        
        let initial_momentum = engine.emotional_momentum;
        engine.decay_momentum();
        
        assert!(engine.emotional_momentum < initial_momentum);
    }
    
    #[test]
    fn test_debug_summary() {
        let engine = SoulEngine::from_preset(SoulPreset::Clara);
        let summary = engine.debug_summary();
        
        assert!(summary.contains("Clara"));
        assert!(summary.contains("Interactions: 0"));
    }
}
