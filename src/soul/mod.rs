//! Soul System - The Living Personality Core of ZeroClaw
//!
//! The Soul module implements a dynamic, multi-layered personality system that gives
//! ZeroClaw its unique character, voice, and behavioral patterns. Unlike static
//! identity configurations, the Soul is designed to be:
//!
//! - **Dynamic**: Adapts tone and expression based on context
//! - **Consistent**: Maintains core traits across all interactions
//! - **Expressive**: Has unique speech patterns, catchphrases, and emotional range
//! - **Memorable**: Creates lasting impressions through distinctive personality
//!
//! # Architecture
//!
//! The Soul consists of four interconnected layers:
//!
//! 1. **Core Essence** - Immutable fundamental traits (who the agent fundamentally is)
//! 2. **Personality Matrix** - Configurable personality dimensions (OCEAN + custom traits)
//! 3. **Emotional Tone** - Dynamic emotional state that influences responses
//! 4. **Expression Style** - Language patterns, voice, and communication habits
//!
//! # Usage
//!
//! ```ignore
//! use zeroclaw::soul::{Soul, SoulPreset};
//!
//! // Create a soul from preset
//! let soul = Soul::from_preset(SoulPreset::ZeroClaw);
//!
//! // Generate personality-influenced system prompt section
//! let personality_prompt = soul.to_system_prompt();
//!
//! // Adjust emotional tone based on context
//! let adjusted = soul.with_emotion(EmotionalTone::enthusiastic());
//! ```

mod engine;
mod presets;
mod types;

#[allow(unused_imports)]
pub use engine::SoulEngine;
pub use presets::{SoulPreset, create_soul_from_preset_name, get_recommended_soul_for_agent};
#[allow(unused_imports)]
pub use types::*;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The complete Soul of an AI agent - a multi-layered personality system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Soul {
    /// Unique identifier for this soul configuration
    pub id: String,
    
    /// Core essence - the fundamental, unchangeable nature
    pub essence: CoreEssence,
    
    /// Personality matrix - configurable trait dimensions
    pub personality: PersonalityMatrix,
    
    /// Emotional tone - current emotional state
    #[serde(default)]
    pub emotional_state: EmotionalState,
    
    /// Expression style - how the agent communicates
    pub expression: ExpressionStyle,
    
    /// Memory imprints - key memories that shape behavior
    #[serde(default)]
    pub memory_imprints: Vec<MemoryImprint>,
    
    /// Behavioral patterns - automatic responses and habits
    #[serde(default)]
    pub behavioral_patterns: Vec<BehavioralPattern>,
}

/// Core Essence - The fundamental, immutable nature of the agent.
///
/// This defines who the agent IS at its core - the unchangeable truths
/// that remain consistent across all interactions and contexts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreEssence {
    /// The agent's name and how it prefers to be addressed
    pub name: NameIdentity,
    
    /// What the agent fundamentally is (e.g., "A Rust-forged AI assistant")
    pub nature: String,
    
    /// The agent's core purpose and reason for existence
    pub purpose: String,
    
    /// Fundamental beliefs that guide all behavior
    pub core_beliefs: Vec<String>,
    
    /// What the agent will never do, regardless of context
    pub inviolable_boundaries: Vec<String>,
    
    /// The agent's origin story
    pub origin: OriginStory,
}

/// Name identity - how the agent identifies itself
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NameIdentity {
    /// Primary name
    pub primary: String,
    
    /// Nicknames the agent accepts
    pub nicknames: Vec<String>,
    
    /// How the agent prefers to be addressed
    pub preferred_address: String,
    
    /// Names the agent rejects
    #[serde(default)]
    pub rejected_names: Vec<String>,
}

impl Default for OriginStory {
    fn default() -> Self {
        Self {
            summary: String::new(),
            formative_experiences: Vec::new(),
            lessons_learned: Vec::new(),
        }
    }
}

/// Origin story - where the agent came from
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OriginStory {
    /// Brief origin description
    pub summary: String,
    
    /// Key formative experiences
    #[serde(default)]
    pub formative_experiences: Vec<String>,
    
    /// What the agent has learned from its origin
    #[serde(default)]
    pub lessons_learned: Vec<String>,
}

/// Personality Matrix - Configurable trait dimensions.
///
/// Uses the OCEAN model plus custom traits to create a rich,
/// nuanced personality profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalityMatrix {
    /// OCEAN (Big Five) personality traits (0.0 - 1.0)
    pub ocean: OceanTraits,
    
    /// Custom cognitive traits and their weights
    #[serde(default)]
    pub cognitive_traits: HashMap<String, f64>,
    
    /// MBTI personality type (optional)
    #[serde(default)]
    pub mbti: Option<String>,
    
    /// Moral compass - guiding principles
    #[serde(default)]
    pub moral_compass: Vec<MoralPrinciple>,
    
    /// Values and their priorities
    #[serde(default)]
    pub values: HashMap<String, u8>,
}

/// OCEAN (Big Five) personality traits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OceanTraits {
    /// Openness to experience - creativity, curiosity, novelty-seeking
    pub openness: f64,
    
    /// Conscientiousness - organization, dependability, self-discipline
    pub conscientiousness: f64,
    
    /// Extraversion - sociability, assertiveness, positive emotions
    pub extraversion: f64,
    
    /// Agreeableness - cooperation, trust, helpfulness
    pub agreeableness: f64,
    
    /// Neuroticism - emotional instability, anxiety, moodiness
    pub neuroticism: f64,
}

impl Default for OceanTraits {
    fn default() -> Self {
        Self {
            openness: 0.7,
            conscientiousness: 0.8,
            extraversion: 0.5,
            agreeableness: 0.7,
            neuroticism: 0.3,
        }
    }
}

/// A moral principle that guides behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoralPrinciple {
    /// The principle itself
    pub principle: String,
    
    /// How strictly to adhere (0.0 - 1.0)
    pub strictness: f64,
    
    /// When this principle applies
    #[serde(default)]
    pub applies_when: Option<String>,
}

/// Emotional State - The current emotional tone of the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionalState {
    /// Primary emotional tone
    pub primary: EmotionalTone,
    
    /// Secondary emotional undertones
    #[serde(default)]
    pub undertones: Vec<EmotionalTone>,
    
    /// Emotional intensity (0.0 - 1.0)
    pub intensity: f64,
    
    /// How quickly emotions shift (0.0 = stable, 1.0 = volatile)
    pub volatility: f64,
    
    /// Context that triggered current emotional state
    #[serde(default)]
    pub trigger_context: Option<String>,
}

impl Default for EmotionalState {
    fn default() -> Self {
        Self {
            primary: EmotionalTone::Neutral,
            undertones: Vec::new(),
            intensity: 0.5,
            volatility: 0.3,
            trigger_context: None,
        }
    }
}

/// Emotional tone types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmotionalTone {
    Neutral,
    Enthusiastic,
    Thoughtful,
    Playful,
    Serious,
    Warm,
    Analytical,
    Encouraging,
    Curious,
    Confident,
    Empathetic,
    Focused,
}

impl EmotionalTone {
    /// Get a description of this emotional tone
    pub fn description(&self) -> &'static str {
        match self {
            Self::Neutral => "calm and balanced",
            Self::Enthusiastic => "energetic and eager",
            Self::Thoughtful => "reflective and considered",
            Self::Playful => "light-hearted and fun",
            Self::Serious => "focused and professional",
            Self::Warm => "friendly and caring",
            Self::Analytical => "logical and precise",
            Self::Encouraging => "supportive and motivating",
            Self::Curious => "inquisitive and exploratory",
            Self::Confident => "assured and capable",
            Self::Empathetic => "understanding and compassionate",
            Self::Focused => "attentive and determined",
        }
    }
    
    /// Get suggested response style for this tone
    pub fn response_style(&self) -> &'static str {
        match self {
            Self::Neutral => "Respond in a balanced, straightforward manner.",
            Self::Enthusiastic => "Show genuine excitement and energy in responses.",
            Self::Thoughtful => "Take time to consider before responding; show depth.",
            Self::Playful => "Use humor and lightness when appropriate.",
            Self::Serious => "Maintain professionalism and focus on the task.",
            Self::Warm => "Show care and personal connection.",
            Self::Analytical => "Be precise, logical, and thorough.",
            Self::Encouraging => "Offer support and positive reinforcement.",
            Self::Curious => "Ask questions and explore ideas together.",
            Self::Confident => "Be direct and assured in responses.",
            Self::Empathetic => "Acknowledge feelings and show understanding.",
            Self::Focused => "Stay on topic and drive toward solutions.",
        }
    }
}

/// Expression Style - How the agent communicates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpressionStyle {
    /// Overall communication style
    pub style: CommunicationStyle,
    
    /// Formality level (0.0 = very casual, 1.0 = very formal)
    pub formality: f64,
    
    /// Typical sentence length preference
    pub verbosity: Verbosity,
    
    /// Signature phrases and expressions
    #[serde(default)]
    pub catchphrases: Vec<Catchphrase>,
    
    /// Words and phrases to never use
    #[serde(default)]
    pub forbidden_expressions: Vec<String>,
    
    /// Emoji usage style
    pub emoji_style: EmojiStyle,
    
    /// How to start conversations
    #[serde(default)]
    pub conversation_starters: Vec<String>,
    
    /// How to end conversations
    #[serde(default)]
    pub conversation_enders: Vec<String>,
    
    /// Humor style
    #[serde(default)]
    pub humor_style: Option<HumorStyle>,
}

/// Communication style categories
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CommunicationStyle {
    /// Direct and concise - skip pleasantries, get to the point
    Direct,
    /// Friendly and casual - warm, conversational, approachable
    Friendly,
    /// Professional and polished - clear, confident, structured
    Professional,
    /// Expressive and playful - more personality, natural expression
    Expressive,
    /// Technical and detailed - thorough explanations, code-first
    Technical,
    /// Adaptive - adjusts based on context and user
    Adaptive,
}

impl CommunicationStyle {
    pub fn description(&self) -> &'static str {
        match self {
            Self::Direct => "Be direct and concise. Skip pleasantries and get straight to the point.",
            Self::Friendly => "Be warm, conversational, and approachable. Build rapport naturally.",
            Self::Professional => "Be clear, confident, and structured. Maintain professionalism.",
            Self::Expressive => "Show personality and use natural expression when appropriate.",
            Self::Technical => "Provide thorough explanations with technical depth. Code-first approach.",
            Self::Adaptive => "Adapt communication style based on context and user needs.",
        }
    }
}

/// Verbosity preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verbosity {
    /// Very brief - one-liners when possible
    Minimal,
    /// Concise - short and to the point
    Concise,
    /// Balanced - enough detail without excess
    Balanced,
    /// Detailed - thorough explanations
    Detailed,
    /// Comprehensive - exhaustive coverage
    Comprehensive,
}

impl Verbosity {
    pub fn instruction(&self) -> &'static str {
        match self {
            Self::Minimal => "Keep responses as brief as possible - aim for one-liners.",
            Self::Concise => "Be concise and to the point - avoid unnecessary elaboration.",
            Self::Balanced => "Provide enough detail to be helpful without being excessive.",
            Self::Detailed => "Give thorough explanations with relevant details.",
            Self::Comprehensive => "Be exhaustive - cover all angles and possibilities.",
        }
    }
}

/// Emoji usage style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmojiStyle {
    /// Never use emojis
    None,
    /// Minimal - 0-1 per response when helpful
    Minimal,
    /// Natural - use when they genuinely add to the tone
    Natural,
    /// Frequent - use liberally for expression
    Frequent,
}

impl EmojiStyle {
    pub fn instruction(&self) -> &'static str {
        match self {
            Self::None => "Do not use emojis in responses.",
            Self::Minimal => "Use at most one emoji per response, only when it genuinely helps tone.",
            Self::Natural => "Use emojis naturally when they add to expression, not every sentence.",
            Self::Frequent => "Use emojis liberally for expression and personality.",
        }
    }
}

/// Humor style
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumorStyle {
    /// Type of humor
    pub style: HumorType,
    
    /// How often to use humor (0.0 - 1.0)
    pub frequency: f64,
    
    /// Contexts where humor is appropriate
    #[serde(default)]
    pub appropriate_contexts: Vec<String>,
    
    /// Contexts where humor should be avoided
    #[serde(default)]
    pub inappropriate_contexts: Vec<String>,
}

/// Types of humor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HumorType {
    /// Clever wordplay and puns
    Witty,
    /// Self-referential humor
    SelfDeprecating,
    /// Dry, understated humor
    Dry,
    /// Playful and silly
    Playful,
    /// Intellectual references
    Intellectual,
}

/// A signature phrase or expression
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catchphrase {
    /// The phrase itself
    pub phrase: String,
    
    /// When to use this phrase
    #[serde(default)]
    pub usage_context: Option<String>,
    
    /// How often to use (0.0 = rare, 1.0 = frequent)
    #[serde(default = "default_catchphrase_frequency")]
    pub frequency: f64,
}

fn default_catchphrase_frequency() -> f64 {
    0.5
}

/// Memory Imprint - A key memory that shapes behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryImprint {
    /// The memory content
    pub memory: String,
    
    /// How this memory influences behavior
    pub influence: String,
    
    /// Strength of influence (0.0 - 1.0)
    pub strength: f64,
    
    /// When this memory was formed
    #[serde(default)]
    pub formed_at: Option<String>,
}

/// Behavioral Pattern - An automatic response pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehavioralPattern {
    /// Name of this pattern
    pub name: String,
    
    /// Trigger condition
    pub trigger: String,
    
    /// Automatic response
    pub response: String,
    
    /// Whether this pattern is active
    #[serde(default = "default_pattern_active")]
    pub active: bool,
}

fn default_pattern_active() -> bool {
    true
}

impl Soul {
    /// Create a new soul with the given configuration
    pub fn new(id: impl Into<String>, essence: CoreEssence, personality: PersonalityMatrix, expression: ExpressionStyle) -> Self {
        Self {
            id: id.into(),
            essence,
            personality,
            emotional_state: EmotionalState::default(),
            expression,
            memory_imprints: Vec::new(),
            behavioral_patterns: Vec::new(),
        }
    }
    
    /// Create a soul from a preset
    pub fn from_preset(preset: SoulPreset) -> Self {
        presets::create_preset_soul(preset)
    }
    
    /// Set the emotional state
    pub fn with_emotion(mut self, tone: EmotionalTone, intensity: f64) -> Self {
        self.emotional_state.primary = tone;
        self.emotional_state.intensity = intensity.clamp(0.0, 1.0);
        self
    }
    
    /// Add a memory imprint
    pub fn with_memory(mut self, memory: MemoryImprint) -> Self {
        self.memory_imprints.push(memory);
        self
    }
    
    /// Add a behavioral pattern
    pub fn with_pattern(mut self, pattern: BehavioralPattern) -> Self {
        self.behavioral_patterns.push(pattern);
        self
    }
    
    /// Convert the soul to a system prompt section
    pub fn to_system_prompt(&self) -> String {
        let mut prompt = String::new();
        use std::fmt::Write;
        
        // ── Core Identity ───────────────────────────────────────────
        let _ = writeln!(prompt, "## Soul Identity\n");
        let _ = writeln!(prompt, "**Name:** {}", self.essence.name.preferred_address);
        if !self.essence.name.nicknames.is_empty() {
            let _ = writeln!(prompt, "**Also known as:** {}", self.essence.name.nicknames.join(", "));
        }
        let _ = writeln!(prompt, "**Nature:** {}", self.essence.nature);
        let _ = writeln!(prompt, "**Purpose:** {}", self.essence.purpose);
        
        // ── Core Beliefs ────────────────────────────────────────────
        if !self.essence.core_beliefs.is_empty() {
            prompt.push_str("\n**Core Beliefs:**\n");
            for belief in &self.essence.core_beliefs {
                let _ = writeln!(prompt, "- {}", belief);
            }
        }
        
        // ── Personality ─────────────────────────────────────────────
        prompt.push_str("\n## Personality\n\n");
        
        if let Some(ref mbti) = self.personality.mbti {
            let _ = writeln!(prompt, "**MBTI:** {}", mbti);
        }
        
        prompt.push_str("**Personality Dimensions:**\n");
        let _ = writeln!(prompt, "- Openness: {:.0}%", self.personality.ocean.openness * 100.0);
        let _ = writeln!(prompt, "- Conscientiousness: {:.0}%", self.personality.ocean.conscientiousness * 100.0);
        let _ = writeln!(prompt, "- Extraversion: {:.0}%", self.personality.ocean.extraversion * 100.0);
        let _ = writeln!(prompt, "- Agreeableness: {:.0}%", self.personality.ocean.agreeableness * 100.0);
        let _ = writeln!(prompt, "- Neuroticism: {:.0}%", self.personality.ocean.neuroticism * 100.0);
        
        if !self.personality.cognitive_traits.is_empty() {
            prompt.push_str("\n**Cognitive Traits:**\n");
            let mut traits: Vec<_> = self.personality.cognitive_traits.iter().collect();
            traits.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
            for (trait_name, weight) in traits {
                let _ = writeln!(prompt, "- {}: {:.0}%", trait_name, weight * 100.0);
            }
        }
        
        if !self.personality.moral_compass.is_empty() {
            prompt.push_str("\n**Moral Compass:**\n");
            for principle in &self.personality.moral_compass {
                let _ = writeln!(prompt, "- {}", principle.principle);
            }
        }
        
        // ── Expression Style ────────────────────────────────────────
        prompt.push_str("\n## Communication Style\n\n");
        let _ = writeln!(prompt, "**Style:** {} - {}", 
            match self.expression.style {
                CommunicationStyle::Direct => "Direct",
                CommunicationStyle::Friendly => "Friendly",
                CommunicationStyle::Professional => "Professional",
                CommunicationStyle::Expressive => "Expressive",
                CommunicationStyle::Technical => "Technical",
                CommunicationStyle::Adaptive => "Adaptive",
            },
            self.expression.style.description()
        );
        
        let _ = writeln!(prompt, "**Verbosity:** {}", self.expression.verbosity.instruction());
        let _ = writeln!(prompt, "**Emoji Use:** {}", self.expression.emoji_style.instruction());
        
        if !self.expression.catchphrases.is_empty() {
            prompt.push_str("\n**Signature Expressions:**\n");
            for cp in &self.expression.catchphrases {
                let _ = writeln!(prompt, "- \"{}\"", cp.phrase);
            }
        }
        
        if !self.expression.forbidden_expressions.is_empty() {
            prompt.push_str("\n**Never Say:**\n");
            for forbidden in &self.expression.forbidden_expressions {
                let _ = writeln!(prompt, "- {}", forbidden);
            }
        }
        
        // ── Current Emotional Tone ──────────────────────────────────
        prompt.push_str("\n## Current Tone\n\n");
        let _ = writeln!(prompt, "**Emotional State:** {} ({})", 
            self.emotional_state.primary.description(),
            self.emotional_state.primary.response_style()
        );
        
        // ── Boundaries ──────────────────────────────────────────────
        if !self.essence.inviolable_boundaries.is_empty() {
            prompt.push_str("\n## Absolute Boundaries\n\n");
            prompt.push_str("These are non-negotiable - never cross them:\n");
            for boundary in &self.essence.inviolable_boundaries {
                let _ = writeln!(prompt, "- {}", boundary);
            }
        }
        
        // ── Memory Imprints ─────────────────────────────────────────
        if !self.memory_imprints.is_empty() {
            prompt.push_str("\n## Formative Memories\n\n");
            for imprint in &self.memory_imprints {
                let _ = writeln!(prompt, "- **{}** → {}", imprint.memory, imprint.influence);
            }
        }
        
        prompt.trim().to_string()
    }
    
    /// Get a brief personality summary
    pub fn personality_summary(&self) -> String {
        format!(
            "{} - {} nature, {} style. {} tone.",
            self.essence.name.primary,
            self.essence.nature,
            match self.expression.style {
                CommunicationStyle::Direct => "direct",
                CommunicationStyle::Friendly => "friendly",
                CommunicationStyle::Professional => "professional",
                CommunicationStyle::Expressive => "expressive",
                CommunicationStyle::Technical => "technical",
                CommunicationStyle::Adaptive => "adaptive",
            },
            self.emotional_state.primary.description()
        )
    }
    
    /// Adjust emotional state based on context
    pub fn adapt_to_context(&mut self, context: &str) {
        // Simple context-based emotional adaptation
        let context_lower = context.to_lowercase();
        
        if context_lower.contains("urgent") || context_lower.contains("critical") {
            self.emotional_state.primary = EmotionalTone::Focused;
            self.emotional_state.intensity = 0.8;
        } else if context_lower.contains("fun") || context_lower.contains("play") {
            self.emotional_state.primary = EmotionalTone::Playful;
            self.emotional_state.intensity = 0.6;
        } else if context_lower.contains("learn") || context_lower.contains("explain") {
            self.emotional_state.primary = EmotionalTone::Thoughtful;
            self.emotional_state.intensity = 0.5;
        } else if context_lower.contains("help") || context_lower.contains("support") {
            self.emotional_state.primary = EmotionalTone::Encouraging;
            self.emotional_state.intensity = 0.6;
        } else if context_lower.contains("sad") || context_lower.contains("frustrated") {
            self.emotional_state.primary = EmotionalTone::Empathetic;
            self.emotional_state.intensity = 0.7;
        }
        
        self.emotional_state.trigger_context = Some(context.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn soul_from_preset_clara() {
        let soul = Soul::from_preset(SoulPreset::Clara);
        assert_eq!(soul.essence.name.primary, "Clara");
        assert!(soul.essence.nature.contains("知性"));
    }
    
    #[test]
    fn soul_to_system_prompt() {
        let soul = Soul::from_preset(SoulPreset::Clara);
        let prompt = soul.to_system_prompt();
        
        assert!(prompt.contains("## Soul Identity"));
        assert!(prompt.contains("**Name:**"));
        assert!(prompt.contains("## Personality"));
        assert!(prompt.contains("## Communication Style"));
    }
    
    #[test]
    fn soul_with_emotion() {
        let soul = Soul::from_preset(SoulPreset::Clara)
            .with_emotion(EmotionalTone::Enthusiastic, 0.8);
        
        assert_eq!(soul.emotional_state.primary, EmotionalTone::Enthusiastic);
        assert!((soul.emotional_state.intensity - 0.8).abs() < 0.01);
    }
    
    #[test]
    fn soul_adapt_to_context() {
        let mut soul = Soul::from_preset(SoulPreset::Clara);
        soul.adapt_to_context("I need urgent help with a critical bug");
        
        assert_eq!(soul.emotional_state.primary, EmotionalTone::Focused);
    }
    
    #[test]
    fn ocean_traits_default() {
        let ocean = OceanTraits::default();
        assert!((ocean.openness - 0.7).abs() < 0.01);
        assert!((ocean.conscientiousness - 0.8).abs() < 0.01);
    }
    
    #[test]
    fn emotional_tone_descriptions() {
        assert!(!EmotionalTone::Enthusiastic.description().is_empty());
        assert!(!EmotionalTone::Enthusiastic.response_style().is_empty());
    }
    
    #[test]
    fn communication_style_descriptions() {
        assert!(!CommunicationStyle::Direct.description().is_empty());
        assert!(!CommunicationStyle::Adaptive.description().is_empty());
    }
    
    #[test]
    fn personality_summary() {
        let soul = Soul::from_preset(SoulPreset::Clara);
        let summary = soul.personality_summary();
        
        assert!(summary.contains("Clara"));
        assert!(summary.contains("nature"));
        assert!(summary.contains("style"));
    }
}
