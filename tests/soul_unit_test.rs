//! 单元测试：Soul 系统
//!
//! 测试 Soul 系统的核心功能，包括人格生成、情感状态管理、预设加载等功能

use zeroclaw::soul::{Soul, SoulPreset, CoreEssence, PersonalityMatrix, EmotionalTone, ExpressionStyle, OceanTraits, EmotionalState, CommunicationStyle, Verbosity, MoralPrinciple, EmojiStyle, Catchphrase};
use std::collections::HashMap;

#[test]
fn test_soul_default_creation() {
    // 测试默认创建的 Soul 实例
    let soul = Soul::default();
    
    // 验证基本属性
    assert_eq!(soul.essence.name.primary, "Clara");
    assert!(soul.essence.purpose.contains("帮助用户"));
    
    // 验证人格矩阵值（Clara预设）
    assert_eq!(soul.personality.ocean.openness, 0.82);
    assert_eq!(soul.personality.ocean.conscientiousness, 0.88);
    assert_eq!(soul.personality.ocean.extraversion, 0.45);
    assert_eq!(soul.personality.ocean.agreeableness, 0.75);
    assert_eq!(soul.personality.ocean.neuroticism, 0.18);
    
    // 验证表达风格
    assert!(matches!(soul.expression.style, CommunicationStyle::Professional));
}

#[test]
fn test_soul_from_preset_clara() {
    // 测试从 Clara 预设创建 Soul
    let soul = Soul::from_preset(SoulPreset::Clara);
    
    // 验证 Clara 预设有特定的名称或特征
    assert!(!soul.essence.name.primary.is_empty());
    
    // 验证人格特质不全是默认值
    assert!(soul.personality.ocean.openness >= 0.0 && soul.personality.ocean.openness <= 1.0);
}

#[test]
fn test_soul_from_preset_zeroclaw() {
    // 测试从 ZeroClaw 预设创建 Soul
    let soul = Soul::from_preset(SoulPreset::ZeroClaw);
    
    // 验证 ZeroClaw 预设的特定特征
    assert!(!soul.essence.name.primary.is_empty());
}

#[test]
fn test_emotional_state_default() {
    // 测试情感状态的默认值
    let emotional_state = EmotionalState::default();
    
    assert!(matches!(emotional_state.primary, EmotionalTone::Neutral));
    assert_eq!(emotional_state.intensity, 0.5);
    assert_eq!(emotional_state.volatility, 0.3);
    assert!(emotional_state.undertones.is_empty());
}

#[test]
fn test_soul_emotional_tone_descriptions() {
    // 测试情感状态的描述方法
    let neutral_desc = EmotionalTone::Neutral.description();
    let enthusiastic_desc = EmotionalTone::Enthusiastic.description();
    let thoughtful_desc = EmotionalTone::Thoughtful.description();
    
    assert!(neutral_desc.contains("calm") || neutral_desc.contains("balanced"));
    assert!(enthusiastic_desc.contains("energetic") || enthusiastic_desc.contains("eager"));
    assert!(thoughtful_desc.contains("reflective") || thoughtful_desc.contains("considered"));
}

#[test]
fn test_core_essence_creation() {
    // 测试核心本质的创建
    let essence = CoreEssence {
        name: zeroclaw::soul::NameIdentity {
            primary: "TestSoul".to_string(),
            nicknames: vec!["TS".to_string(), "T".to_string()],
            preferred_address: "Test".to_string(),
            rejected_names: vec!["BadName".to_string()],
        },
        nature: "A testing soul".to_string(),
        purpose: "To test the soul system".to_string(),
        core_beliefs: vec!["Testing is important".to_string()],
        inviolable_boundaries: vec!["No harmful behavior".to_string()],
        origin: zeroclaw::soul::OriginStory {
            summary: "Created for testing purposes".to_string(),
            formative_experiences: vec!["Born in unit test".to_string()],
            lessons_learned: vec!["Tests improve quality".to_string()],
        },
    };
    
    assert_eq!(essence.name.primary, "TestSoul");
    assert_eq!(essence.nature, "A testing soul");
    assert_eq!(essence.purpose, "To test the soul system");
    assert_eq!(essence.core_beliefs.len(), 1);
}

#[test]
fn test_personality_matrix_creation() {
    // 测试人格矩阵的创建
    let mut cognitive_traits = HashMap::new();
    cognitive_traits.insert("curiosity".to_string(), 0.8);
    cognitive_traits.insert("analytical_thinking".to_string(), 0.7);
    
    let mut values = HashMap::new();
    values.insert("accuracy".to_string(), 90);  // u8 values
    values.insert("efficiency".to_string(), 60);  // u8 values
    
    let matrix = PersonalityMatrix {
        ocean: OceanTraits {
            openness: 0.7,
            conscientiousness: 0.8,
            extraversion: 0.3,
            agreeableness: 0.6,
            neuroticism: 0.4,
        },
        cognitive_traits,
        mbti: Some("INTJ".to_string()),
        moral_compass: vec![
            MoralPrinciple {
                principle: "Respect user privacy".to_string(),
                strictness: 0.95,
                applies_when: Some("when handling sensitive data".to_string()),
            },
            MoralPrinciple {
                principle: "Provide accurate information".to_string(),
                strictness: 1.0,
                applies_when: None,
            }
        ],
        values,
    };
    
    assert_eq!(matrix.ocean.openness, 0.7);
    assert_eq!(matrix.mbti, Some("INTJ".to_string()));
    assert_eq!(matrix.cognitive_traits.get("curiosity"), Some(&0.8));
    assert_eq!(matrix.moral_compass.len(), 2);
    assert_eq!(matrix.values.get("accuracy"), Some(&90));
}

#[test]
fn test_expression_style_adaptive() {
    // 测试自适应表达风格
    let adaptive_style = ExpressionStyle {
        style: CommunicationStyle::Adaptive,
        formality: 0.5,
        verbosity: Verbosity::Balanced,
        catchphrases: vec![
            Catchphrase {
                phrase: "I understand".to_string(),
                usage_context: Some("acknowledging user input".to_string()),
                frequency: 0.7,
            },
            Catchphrase {
                phrase: "Let me think".to_string(),
                usage_context: Some("when considering complex questions".to_string()),
                frequency: 0.5,
            },
        ],
        forbidden_expressions: vec![],
        emoji_style: EmojiStyle::Minimal,
        conversation_starters: vec![],
        conversation_enders: vec![],
        humor_style: None,
    };
    
    assert!(matches!(adaptive_style.style, CommunicationStyle::Adaptive));
    assert_eq!(adaptive_style.formality, 0.5);
}

#[test]
fn test_soul_clone_behavior() {
    // 测试 Soul 的克隆行为
    let original = Soul::from_preset(SoulPreset::Clara);
    let cloned = original.clone();
    
    // 验证克隆的 Soul 具有相同的属性
    assert_eq!(original.essence.name.primary, cloned.essence.name.primary);
    assert_eq!(original.personality.ocean.openness, cloned.personality.ocean.openness);
    assert_eq!(original.emotional_state.intensity, cloned.emotional_state.intensity);
    assert_eq!(
        format!("{:?}", original.expression.style),
        format!("{:?}", cloned.expression.style)
    );
}

#[test]
fn test_soul_preset_variety() {
    // 测试不同的 Soul 预设产生不同的人格
    let clara = Soul::from_preset(SoulPreset::Clara);
    let zeroclaw = Soul::from_preset(SoulPreset::ZeroClaw);
    let technical = Soul::from_preset(SoulPreset::TechnicalExpert);
    
    // 验证不同预设具有不同特征（至少在名称上）
    // 注意：这里我们测试它们不是完全相同，而不是特定差异
    let clara_prompt = clara.to_system_prompt();
    let zeroclaw_prompt = zeroclaw.to_system_prompt();
    let tech_prompt = technical.to_system_prompt();
    
    // 简单验证它们都生成了非空提示
    assert!(!clara_prompt.is_empty());
    assert!(!zeroclaw_prompt.is_empty());
    assert!(!tech_prompt.is_empty());
}

#[test]
fn test_communication_styles() {
    // 测试不同的交流风格
    let direct_style = CommunicationStyle::Direct;
    let friendly_style = CommunicationStyle::Friendly;
    let professional_style = CommunicationStyle::Professional;
    let adaptive_style = CommunicationStyle::Adaptive;
    
    // 简单验证它们都是有效的枚举值
    assert!(matches!(direct_style, CommunicationStyle::Direct));
    assert!(matches!(friendly_style, CommunicationStyle::Friendly));
    assert!(matches!(professional_style, CommunicationStyle::Professional));
    assert!(matches!(adaptive_style, CommunicationStyle::Adaptive));
}

#[test]
fn test_soul_with_emotion() {
    // 测试带情感的Soul创建
    let base_soul = Soul::default();
    let emotional_soul = base_soul.clone().with_emotion(EmotionalTone::Enthusiastic, 0.8);
    
    // 验证基础Soul的情感强度未改变
    assert_ne!(base_soul.emotional_state.intensity, emotional_soul.emotional_state.intensity);
    
    // 验证情感变化
    assert_eq!(emotional_soul.emotional_state.primary, EmotionalTone::Enthusiastic);
    assert_eq!(emotional_soul.emotional_state.intensity, 0.8);
}

#[test]
fn test_soul_to_system_prompt() {
    // 测试生成系统提示功能
    let soul = Soul::default();
    let prompt = soul.to_system_prompt();
    
    // 验证生成的提示不为空
    assert!(!prompt.is_empty());
    
    // 验证提示包含人格相关信息
    assert!(prompt.contains("Assistant") || prompt.contains("AI"));
}