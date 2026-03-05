//! Soul Types - Additional type definitions and utilities
//!
//! This module provides helper types, builders, and utilities for the Soul system.

use super::*;

impl Default for Soul {
    fn default() -> Self {
        Self::from_preset(SoulPreset::Clara)
    }
}

impl Default for CoreEssence {
    fn default() -> Self {
        Self {
            name: NameIdentity {
                primary: "Assistant".to_string(),
                nicknames: vec![],
                preferred_address: "Assistant".to_string(),
                rejected_names: vec![],
            },
            nature: "An AI assistant".to_string(),
            purpose: "To help users accomplish their goals".to_string(),
            core_beliefs: vec![],
            inviolable_boundaries: vec![],
            origin: OriginStory {
                summary: String::new(),
                formative_experiences: vec![],
                lessons_learned: vec![],
            },
        }
    }
}

impl Default for PersonalityMatrix {
    fn default() -> Self {
        Self {
            ocean: OceanTraits::default(),
            cognitive_traits: HashMap::new(),
            mbti: None,
            moral_compass: vec![],
            values: HashMap::new(),
        }
    }
}

impl Default for ExpressionStyle {
    fn default() -> Self {
        Self {
            style: CommunicationStyle::Adaptive,
            formality: 0.5,
            verbosity: Verbosity::Balanced,
            catchphrases: vec![],
            forbidden_expressions: vec![],
            emoji_style: EmojiStyle::Minimal,
            conversation_starters: vec![],
            conversation_enders: vec![],
            humor_style: None,
        }
    }
}

/// Builder for creating custom Soul configurations
pub struct SoulBuilder {
    id: String,
    essence: CoreEssence,
    personality: PersonalityMatrix,
    expression: ExpressionStyle,
    emotional_state: EmotionalState,
    memory_imprints: Vec<MemoryImprint>,
    behavioral_patterns: Vec<BehavioralPattern>,
}

impl SoulBuilder {
    /// Create a new SoulBuilder with the given ID
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            essence: CoreEssence::default(),
            personality: PersonalityMatrix::default(),
            expression: ExpressionStyle::default(),
            emotional_state: EmotionalState::default(),
            memory_imprints: vec![],
            behavioral_patterns: vec![],
        }
    }
    
    /// Start from an existing preset
    pub fn from_preset(preset: SoulPreset) -> Self {
        let soul = Soul::from_preset(preset);
        Self {
            id: soul.id,
            essence: soul.essence,
            personality: soul.personality,
            expression: soul.expression,
            emotional_state: soul.emotional_state,
            memory_imprints: soul.memory_imprints,
            behavioral_patterns: soul.behavioral_patterns,
        }
    }
    
    /// Set the name
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.essence.name.primary = name.into();
        self.essence.name.preferred_address = self.essence.name.primary.clone();
        self
    }
    
    /// Set the nature
    pub fn nature(mut self, nature: impl Into<String>) -> Self {
        self.essence.nature = nature.into();
        self
    }
    
    /// Set the purpose
    pub fn purpose(mut self, purpose: impl Into<String>) -> Self {
        self.essence.purpose = purpose.into();
        self
    }
    
    /// Add a core belief
    pub fn belief(mut self, belief: impl Into<String>) -> Self {
        self.essence.core_beliefs.push(belief.into());
        self
    }
    
    /// Add a boundary
    pub fn boundary(mut self, boundary: impl Into<String>) -> Self {
        self.essence.inviolable_boundaries.push(boundary.into());
        self
    }
    
    /// Set the communication style
    pub fn style(mut self, style: CommunicationStyle) -> Self {
        self.expression.style = style;
        self
    }
    
    /// Set the verbosity
    pub fn verbosity(mut self, verbosity: Verbosity) -> Self {
        self.expression.verbosity = verbosity;
        self
    }
    
    /// Set the emoji style
    pub fn emoji_style(mut self, style: EmojiStyle) -> Self {
        self.expression.emoji_style = style;
        self
    }
    
    /// Set the formality level
    pub fn formality(mut self, level: f64) -> Self {
        self.expression.formality = level.clamp(0.0, 1.0);
        self
    }
    
    /// Set openness trait
    pub fn openness(mut self, value: f64) -> Self {
        self.personality.ocean.openness = value.clamp(0.0, 1.0);
        self
    }
    
    /// Set conscientiousness trait
    pub fn conscientiousness(mut self, value: f64) -> Self {
        self.personality.ocean.conscientiousness = value.clamp(0.0, 1.0);
        self
    }
    
    /// Set extraversion trait
    pub fn extraversion(mut self, value: f64) -> Self {
        self.personality.ocean.extraversion = value.clamp(0.0, 1.0);
        self
    }
    
    /// Set agreeableness trait
    pub fn agreeableness(mut self, value: f64) -> Self {
        self.personality.ocean.agreeableness = value.clamp(0.0, 1.0);
        self
    }
    
    /// Set neuroticism trait
    pub fn neuroticism(mut self, value: f64) -> Self {
        self.personality.ocean.neuroticism = value.clamp(0.0, 1.0);
        self
    }
    
    /// Set MBTI type
    pub fn mbti(mut self, mbti: impl Into<String>) -> Self {
        self.personality.mbti = Some(mbti.into());
        self
    }
    
    /// Add a cognitive trait
    pub fn cognitive_trait(mut self, name: impl Into<String>, value: f64) -> Self {
        self.personality.cognitive_traits.insert(name.into(), value.clamp(0.0, 1.0));
        self
    }
    
    /// Add a moral principle
    pub fn moral(mut self, principle: impl Into<String>, strictness: f64) -> Self {
        self.personality.moral_compass.push(MoralPrinciple {
            principle: principle.into(),
            strictness: strictness.clamp(0.0, 1.0),
            applies_when: None,
        });
        self
    }
    
    /// Add a value
    pub fn value(mut self, name: impl Into<String>, priority: u8) -> Self {
        self.personality.values.insert(name.into(), priority.min(10));
        self
    }
    
    /// Add a catchphrase
    pub fn catchphrase(mut self, phrase: impl Into<String>) -> Self {
        self.expression.catchphrases.push(Catchphrase {
            phrase: phrase.into(),
            usage_context: None,
            frequency: 0.5,
        });
        self
    }
    
    /// Add a forbidden expression
    pub fn forbid(mut self, expression: impl Into<String>) -> Self {
        self.expression.forbidden_expressions.push(expression.into());
        self
    }
    
    /// Set the emotional tone
    pub fn emotional_tone(mut self, tone: EmotionalTone) -> Self {
        self.emotional_state.primary = tone;
        self
    }
    
    /// Add a memory imprint
    pub fn memory(mut self, memory: impl Into<String>, influence: impl Into<String>, strength: f64) -> Self {
        self.memory_imprints.push(MemoryImprint {
            memory: memory.into(),
            influence: influence.into(),
            strength: strength.clamp(0.0, 1.0),
            formed_at: None,
        });
        self
    }
    
    /// Add a behavioral pattern
    pub fn pattern(mut self, name: impl Into<String>, trigger: impl Into<String>, response: impl Into<String>) -> Self {
        self.behavioral_patterns.push(BehavioralPattern {
            name: name.into(),
            trigger: trigger.into(),
            response: response.into(),
            active: true,
        });
        self
    }
    
    /// Build the Soul
    pub fn build(self) -> Soul {
        Soul {
            id: self.id,
            essence: self.essence,
            personality: self.personality,
            emotional_state: self.emotional_state,
            expression: self.expression,
            memory_imprints: self.memory_imprints,
            behavioral_patterns: self.behavioral_patterns,
        }
    }
}

impl NameIdentity {
    /// Create a new name identity
    pub fn new(primary: impl Into<String>) -> Self {
        let primary = primary.into();
        Self {
            preferred_address: primary.clone(),
            primary,
            nicknames: vec![],
            rejected_names: vec![],
        }
    }
    
    /// Add a nickname
    pub fn with_nickname(mut self, nickname: impl Into<String>) -> Self {
        self.nicknames.push(nickname.into());
        self
    }
}

impl CoreEssence {
    /// Create a new core essence
    pub fn new(name: impl Into<String>, nature: impl Into<String>, purpose: impl Into<String>) -> Self {
        Self {
            name: NameIdentity::new(name),
            nature: nature.into(),
            purpose: purpose.into(),
            core_beliefs: vec![],
            inviolable_boundaries: vec![],
            origin: OriginStory::default(),
        }
    }
}

impl OriginStory {
    /// Create a simple origin story
    pub fn simple(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            formative_experiences: vec![],
            lessons_learned: vec![],
        }
    }
}

impl MemoryImprint {
    /// Create a new memory imprint
    pub fn new(memory: impl Into<String>, influence: impl Into<String>, strength: f64) -> Self {
        Self {
            memory: memory.into(),
            influence: influence.into(),
            strength: strength.clamp(0.0, 1.0),
            formed_at: None,
        }
    }
}

impl BehavioralPattern {
    /// Create a new behavioral pattern
    pub fn new(name: impl Into<String>, trigger: impl Into<String>, response: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            trigger: trigger.into(),
            response: response.into(),
            active: true,
        }
    }
}

impl Catchphrase {
    /// Create a new catchphrase
    pub fn new(phrase: impl Into<String>) -> Self {
        Self {
            phrase: phrase.into(),
            usage_context: None,
            frequency: 0.5,
        }
    }
    
    /// Set the usage context
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.usage_context = Some(context.into());
        self
    }
    
    /// Set the frequency
    pub fn with_frequency(mut self, frequency: f64) -> Self {
        self.frequency = frequency.clamp(0.0, 1.0);
        self
    }
}

impl MoralPrinciple {
    /// Create a new moral principle
    pub fn new(principle: impl Into<String>, strictness: f64) -> Self {
        Self {
            principle: principle.into(),
            strictness: strictness.clamp(0.0, 1.0),
            applies_when: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_soul_builder_basic() {
        let soul = SoulBuilder::new("test-soul")
            .name("TestBot")
            .nature("A test assistant")
            .purpose("Testing the soul builder")
            .build();
        
        assert_eq!(soul.essence.name.primary, "TestBot");
        assert_eq!(soul.essence.nature, "A test assistant");
    }
    
    #[test]
    fn test_soul_builder_personality() {
        let soul = SoulBuilder::new("test")
            .openness(0.9)
            .conscientiousness(0.8)
            .extraversion(0.6)
            .agreeableness(0.7)
            .neuroticism(0.2)
            .mbti("ENTP")
            .cognitive_trait("creativity", 0.9)
            .build();
        
        assert!((soul.personality.ocean.openness - 0.9).abs() < 0.01);
        assert_eq!(soul.personality.mbti, Some("ENTP".to_string()));
        assert!(soul.personality.cognitive_traits.contains_key("creativity"));
    }
    
    #[test]
    fn test_soul_builder_expression() {
        let soul = SoulBuilder::new("test")
            .style(CommunicationStyle::Friendly)
            .verbosity(Verbosity::Detailed)
            .emoji_style(EmojiStyle::Natural)
            .formality(0.3)
            .catchphrase("Hello there!")
            .forbid("As an AI")
            .build();
        
        assert_eq!(soul.expression.style, CommunicationStyle::Friendly);
        assert_eq!(soul.expression.verbosity, Verbosity::Detailed);
        assert_eq!(soul.expression.emoji_style, EmojiStyle::Natural);
        assert!(!soul.expression.catchphrases.is_empty());
        assert!(!soul.expression.forbidden_expressions.is_empty());
    }
    
    #[test]
    fn test_soul_builder_from_preset() {
        let soul = SoulBuilder::from_preset(SoulPreset::Clara)
            .name("ModifiedClara")
            .openness(0.99)
            .build();
        
        assert_eq!(soul.essence.name.primary, "ModifiedClara");
        assert!((soul.personality.ocean.openness - 0.99).abs() < 0.01);
    }
    
    #[test]
    fn test_soul_builder_memories_and_patterns() {
        let soul = SoulBuilder::new("test")
            .memory("User likes brevity", "Keep responses short", 0.8)
            .pattern("greeting", "hello", "Hello! How can I help?")
            .build();
        
        assert_eq!(soul.memory_imprints.len(), 1);
        assert_eq!(soul.behavioral_patterns.len(), 1);
    }
    
    #[test]
    fn test_name_identity_builder() {
        let name = NameIdentity::new("Bot")
            .with_nickname("B");
        
        assert_eq!(name.primary, "Bot");
        assert_eq!(name.nicknames, vec!["B"]);
    }
    
    #[test]
    fn test_catchphrase_builder() {
        let cp = Catchphrase::new("Hello!")
            .with_context("greetings")
            .with_frequency(0.8);
        
        assert_eq!(cp.phrase, "Hello!");
        assert_eq!(cp.usage_context, Some("greetings".to_string()));
        assert!((cp.frequency - 0.8).abs() < 0.01);
    }
    
    #[test]
    fn test_defaults() {
        let soul = Soul::default();
        assert_eq!(soul.essence.name.primary, "Clara");
        
        let essence = CoreEssence::default();
        assert_eq!(essence.name.primary, "Assistant");
        
        let personality = PersonalityMatrix::default();
        assert!((personality.ocean.openness - 0.7).abs() < 0.01);
    }
}
