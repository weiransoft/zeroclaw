use std::collections::HashSet;
use std::sync::Arc;
use zeroclaw::swarm::agent_task::{
    AgentTaskManager, AgentTaskStatus, DependencyGraph, TaskSource, TeamTaskSynchronizer,
};
use zeroclaw::swarm::consensus::{ConsensusManager, ConsensusProposal};
use zeroclaw::swarm::engine::{WorkflowEngine, WorkflowStatus, WorkflowStore};
use zeroclaw::swarm::llm_coordinator::{
    AgentLLMClient, CoordinatorConfig, LLMConcurrencyCoordinator,
};
use zeroclaw::swarm::phase::{
    Deliverable, DeliverableType, PhaseCompletionCriteria, PhaseType, WorkflowPhase,
};
use zeroclaw::swarm::planning::{
    AdjustmentContext, DynamicPlanningEngine, Experience, ExperienceStore, KnowledgeBase,
    KnowledgeEntry, TaskComplexity, WorkflowConstraints,
};

fn now_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn create_test_phase(id: &str, name: &str) -> WorkflowPhase {
    WorkflowPhase::new(
        id,
        name,
        "Test phase description",
        PhaseType::Development {
            dev_progress: 0.0,
            code_review_rate: 0.0,
            test_coverage: 0.0,
        },
    )
}

#[tokio::test]
async fn test_complete_scrum_workflow_scenario() {
    let temp_dir = std::env::temp_dir();
    let store = Arc::new(WorkflowStore::new());
    let consensus = Arc::new(ConsensusManager::new(&temp_dir));
    let engine = WorkflowEngine::new(store.clone(), consensus);

    let phases = vec![
        create_test_phase("phase-1", "需求分析"),
        create_test_phase("phase-2", "架构设计"),
        create_test_phase("phase-3", "任务分解"),
        create_test_phase("phase-4", "开发实现"),
        create_test_phase("phase-5", "测试验证"),
        create_test_phase("phase-6", "评审交付"),
        create_test_phase("phase-7", "回顾改进"),
    ];

    let workflow = engine
        .create_workflow("用户认证功能", "开发用户认证模块，包括登录、注册、密码重置", phases)
        .await
        .unwrap();

    assert_eq!(workflow.phases.len(), 7);
    assert_eq!(workflow.status, WorkflowStatus::Pending);

    engine.start_workflow(&workflow.id).await.unwrap();
    let started = store.get_workflow(&workflow.id).await.unwrap();
    assert_eq!(started.status, WorkflowStatus::InProgress);

    let deliverables = vec![Deliverable {
        id: "d1".to_string(),
        name: "PRD.md".to_string(),
        description: "产品需求文档".to_string(),
        deliverable_type: DeliverableType::Document,
        content: Some("# 产品需求\n\n## 功能需求\n...".to_string()),
        file_path: None,
        is_knowledge: true,
        created_at: now_unix(),
    }];

    let transition = engine.advance_phase(&workflow.id, deliverables).await.unwrap();
    assert!(transition.from_phase.is_some());

    let final_workflow = store.get_workflow(&workflow.id).await.unwrap();
    assert!(final_workflow.overall_progress() > 0.0);
}

#[tokio::test]
async fn test_technical_decision_consensus_scenario() {
    let temp_dir = std::env::temp_dir();
    let manager = ConsensusManager::new(&temp_dir);

    let proposal = ConsensusProposal {
        task_id: "task-001".to_string(),
        topic: "数据库选型".to_string(),
        description: "请团队投票选择项目使用的数据库".to_string(),
        proposed_by: "architect".to_string(),
        participants: vec![
            "frontend_developer".to_string(),
            "backend_developer".to_string(),
            "qa_engineer".to_string(),
        ],
        timeout_seconds: 3600,
    };

    let result = manager.initiate_consensus(proposal, "zh");
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_workflow_dynamic_adjustment_scenario() {
    let workflow_store = Arc::new(WorkflowStore::new());
    let knowledge_base = Arc::new(KnowledgeBase::new());
    let experience_store = Arc::new(ExperienceStore::new());

    let engine = DynamicPlanningEngine::new(
        workflow_store.clone(),
        knowledge_base,
        experience_store,
    );

    let constraints = WorkflowConstraints::default();
    let workflow = engine
        .generate_workflow("开发支付功能", &constraints)
        .await
        .unwrap();

    assert!(!workflow.phases.is_empty());

    let mut wf = zeroclaw::swarm::engine::Workflow::new("test-adjustment-1", &workflow.name, "test");
    wf.status = WorkflowStatus::InProgress;
    wf.started_at = Some(now_unix() - 3600);
    wf.phases = workflow.phases.iter().map(|p| p.name.clone()).collect();
    workflow_store.create_workflow(&wf).await.unwrap();

    let context = AdjustmentContext {
        current_progress: 0.3,
        blockers: vec!["第三方 API 不稳定".to_string()],
        team_availability: std::collections::HashMap::new(),
        recent_changes: vec![],
    };

    let adjustment = engine
        .adjust_workflow("test-adjustment-1", "技术难题需要调整方案", &context)
        .await
        .unwrap();

    assert!(adjustment.updated_estimate_hours > 0.0);
}

#[tokio::test]
async fn test_knowledge_sharing_scenario() {
    let knowledge_base = Arc::new(KnowledgeBase::new());
    let experience_store = Arc::new(ExperienceStore::new());

    let knowledge = KnowledgeEntry {
        id: "kn-001".to_string(),
        title: "JWT 认证最佳实践".to_string(),
        summary: "使用 JWT 进行用户认证的标准流程".to_string(),
        content: "使用 JWT 进行用户认证的标准流程和注意事项...".to_string(),
        category: "Technical".to_string(),
        tags: vec!["JWT".to_string(), "认证".to_string()],
        created_at: now_unix(),
    };
    knowledge_base.add_entry(knowledge).await;

    let experience = Experience {
        id: "exp-001".to_string(),
        title: "高并发场景优化经验".to_string(),
        description: "在处理高并发时，使用连接池和缓存可以显著提升性能".to_string(),
        content: "详细描述了在电商大促期间如何优化系统性能...".to_string(),
        tags: vec!["高并发".to_string(), "性能优化".to_string()],
        rating: 4.5,
        created_at: now_unix(),
    };
    experience_store.add_experience(experience).await;

    let search_results = knowledge_base.search("认证", 10).await;
    assert_eq!(search_results.len(), 1);
    assert!(search_results[0].title.contains("JWT"));

    let exp_results = experience_store.search("高并发", 10).await;
    assert_eq!(exp_results.len(), 1);
}

#[tokio::test]
async fn test_multi_agent_collaboration_scenario() {
    let sync = Arc::new(TeamTaskSynchronizer::new());

    let frontend_manager =
        AgentTaskManager::new("frontend_developer").with_team_sync(sync.clone());
    let backend_manager =
        AgentTaskManager::new("backend_developer").with_team_sync(sync.clone());
    let qa_manager = AgentTaskManager::new("qa_engineer").with_team_sync(sync.clone());

    let frontend_task = frontend_manager
        .create_task(
            "实现登录页面",
            "实现用户登录界面",
            zeroclaw::swarm::TaskPriority::High,
            vec![],
            TaskSource::TeamAssigned {
                from: "scrum_master".to_string(),
            },
        )
        .await
        .unwrap();

    let backend_task = backend_manager
        .create_task(
            "实现登录 API",
            "实现 POST /api/login 接口",
            zeroclaw::swarm::TaskPriority::High,
            vec![],
            TaskSource::TeamAssigned {
                from: "scrum_master".to_string(),
            },
        )
        .await
        .unwrap();

    let qa_task = qa_manager
        .create_task(
            "测试登录功能",
            "测试登录功能的正确性和安全性",
            zeroclaw::swarm::TaskPriority::Medium,
            vec![frontend_task.id.clone(), backend_task.id.clone()],
            TaskSource::TeamAssigned {
                from: "scrum_master".to_string(),
            },
        )
        .await
        .unwrap();

    assert_eq!(qa_task.status, AgentTaskStatus::WaitingForDependencies);

    frontend_manager
        .update_task_status(&frontend_task.id, AgentTaskStatus::InProgress)
        .await
        .unwrap();
    frontend_manager
        .update_task_status(&frontend_task.id, AgentTaskStatus::Completed)
        .await
        .unwrap();

    backend_manager
        .update_task_status(&backend_task.id, AgentTaskStatus::InProgress)
        .await
        .unwrap();
    backend_manager
        .update_task_status(&backend_task.id, AgentTaskStatus::Completed)
        .await
        .unwrap();

    let team_view = sync.get_team_view().await;
    assert!(team_view.total_tasks >= 3);
}

#[tokio::test]
async fn test_llm_coordinator_scenario() {
    let config = CoordinatorConfig::default();
    let coordinator = Arc::new(LLMConcurrencyCoordinator::new(config, 5, 60, 100000));

    let client = AgentLLMClient::new("test_agent".to_string(), coordinator.clone());

    let status = client.get_queue_status().await;
    assert_eq!(status.max_concurrent, 5);
    assert_eq!(status.available_slots, 5);

    let stats = coordinator.get_statistics().await;
    assert_eq!(stats.total_requests, 0);
    assert_eq!(stats.success_rate, 0.0);

    let (requests, max_requests, tokens, max_tokens) = coordinator.get_rate_limiter_status();
    assert_eq!(requests, 0);
    assert_eq!(tokens, 0);
    assert!(max_requests > 0);
    assert!(max_tokens > 0);
}

#[tokio::test]
async fn test_dependency_graph_scenario() {
    let mut graph = DependencyGraph::new();

    graph.add_dependency("task-2", "task-1");
    graph.add_dependency("task-3", "task-1");
    graph.add_dependency("task-4", "task-2");
    graph.add_dependency("task-4", "task-3");

    let completed: HashSet<String> = HashSet::new();
    let ready = graph.get_ready_tasks(&completed);
    assert_eq!(ready, vec!["task-1"]);

    let completed: HashSet<String> = vec!["task-1".to_string()].into_iter().collect();
    let ready = graph.get_ready_tasks(&completed);
    assert!(ready.contains(&"task-2".to_string()));
    assert!(ready.contains(&"task-3".to_string()));
    assert!(!ready.contains(&"task-4".to_string()));

    let completed: HashSet<String> =
        vec!["task-1".to_string(), "task-2".to_string(), "task-3".to_string()]
            .into_iter()
            .collect();
    let ready = graph.get_ready_tasks(&completed);
    assert!(ready.contains(&"task-4".to_string()));

    let mut g = DependencyGraph::new();
    g.add_dependency("a", "b");
    g.add_dependency("b", "c");
    g.add_dependency("c", "a");
    let cycle = g.detect_cycle();
    assert!(cycle.is_some());
}

#[tokio::test]
async fn test_phase_completion_criteria_scenario() {
    let criteria = PhaseCompletionCriteria {
        required_tasks: vec!["task-1".to_string(), "task-2".to_string()],
        required_documents: vec!["PRD.md".to_string()],
        required_reviews: vec!["code-review".to_string()],
        required_consensus: vec![],
        quality_metrics: std::collections::HashMap::from([("test_coverage".to_string(), 80.0)]),
    };

    let completed_tasks: HashSet<String> =
        vec!["task-1".to_string(), "task-2".to_string()].into_iter().collect();
    let completed_documents: HashSet<String> = vec!["PRD.md".to_string()].into_iter().collect();
    let completed_reviews: HashSet<String> = vec!["code-review".to_string()].into_iter().collect();
    let completed_consensus: HashSet<String> = HashSet::new();
    let current_metrics =
        std::collections::HashMap::from([("test_coverage".to_string(), 85.0)]);

    let status = criteria.check_completion(
        &completed_tasks,
        &completed_documents,
        &completed_reviews,
        &completed_consensus,
        &current_metrics,
    );

    assert!(status.is_complete);
    assert_eq!(status.progress, 1.0);
    assert!(status.missing_tasks.is_empty());
    assert!(status.missing_documents.is_empty());
}

#[tokio::test]
async fn test_task_decomposition_scenario() {
    let workflow_store = Arc::new(WorkflowStore::new());
    let knowledge_base = Arc::new(KnowledgeBase::new());
    let experience_store = Arc::new(ExperienceStore::new());

    let engine = DynamicPlanningEngine::new(workflow_store, knowledge_base, experience_store);

    let complexity = TaskComplexity {
        technical: 0.8,
        business: 0.6,
        dependencies: 0.4,
    };

    let decomposition = engine.decompose_task("实现用户认证模块", &complexity).await.unwrap();

    assert!(!decomposition.subtasks.is_empty());
    assert!(decomposition.estimated_total_hours > 0.0);

    for subtask in &decomposition.subtasks {
        assert!(!subtask.title.is_empty());
        assert!(subtask.estimated_hours > 0.0);
    }
}

#[tokio::test]
async fn test_workflow_prediction_scenario() {
    let workflow_store = Arc::new(WorkflowStore::new());
    let knowledge_base = Arc::new(KnowledgeBase::new());
    let experience_store = Arc::new(ExperienceStore::new());

    let engine = DynamicPlanningEngine::new(
        workflow_store.clone(),
        knowledge_base,
        experience_store,
    );

    let mut workflow =
        zeroclaw::swarm::engine::Workflow::new("predict-test-1", "测试工作流", "测试");
    workflow.status = WorkflowStatus::InProgress;
    workflow.started_at = Some(now_unix() - 3600);
    workflow.phases = vec!["phase-1".to_string(), "phase-2".to_string()];
    workflow_store.create_workflow(&workflow).await.unwrap();

    let prediction = engine.predict_completion("predict-test-1").await.unwrap();

    assert!(prediction.estimated_completion_time > 0);
    assert!(!prediction.key_factors.is_empty());
    assert!(matches!(prediction.confidence_level.as_str(), "高" | "中" | "低"));
}
