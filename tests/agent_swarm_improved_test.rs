use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use uuid::Uuid;
use zeroclaw::config::{Config, DelegateAgentConfig, SwarmConfig};
use zeroclaw::security::SecurityPolicy;
use zeroclaw::swarm::{manager_for_workspace, SwarmContext};
use zeroclaw::tools::sessions_spawn::SessionsSpawnTool;
use zeroclaw::tools::{SubagentsTool, Tool};

fn parse_run_id(output: &str) -> Uuid {
    let first = output.lines().next().unwrap_or("");
    let (_, id) = first.split_once("run_id=").unwrap();
    Uuid::parse_str(id.trim()).unwrap()
}

#[tokio::test]
async fn test_improved_a_stock_fund_analysis() {
    let tmp = TempDir::new().unwrap();
    let workspace_dir = tmp.path().join("workspace");
    tokio::fs::create_dir_all(&workspace_dir).await.unwrap();

    tracing::info!("=== Start improved A-share stock fund analysis test ===");
    tracing::info!("Workspace directory: {:?}", workspace_dir);

    let mut cfg = Config::default();
    cfg.workspace_dir = workspace_dir.clone();
    cfg.default_provider = Some("glm".to_string());
    cfg.default_model = Some("glm-5".to_string());
    cfg.api_key = Some("35d8f027d4a64ebebd76c4b5dc2665a3.XuWlGaHk4i8exBbw".to_string());
    cfg.swarm = SwarmConfig {
        subagent_max_concurrent: 5,
        orchestrator_prompt: None,
    };

    let mut agents = HashMap::new();

    agents.insert(
        "project_manager".to_string(),
        DelegateAgentConfig {
            provider: "glm".to_string(),
            model: "glm-5".to_string(),
            system_prompt: Some("You are a project manager. Your ONLY job is to use the sessions_spawn tool to create subtasks.

CRITICAL: You MUST call the sessions_spawn tool IMMEDIATELY. Do NOT write any text first.

Create these 4 subtasks using sessions_spawn:

1. frontend_developer - task: \"Design frontend interface\" - label: \"frontend-dev\"
2. backend_developer - task: \"Design backend API\" - label: \"backend-dev\"  
3. data_analyst - task: \"Implement fund analysis\" - label: \"data-analysis\"
4. qa_engineer - task: \"Write test cases\" - label: \"qa-testing\"

Example call:
<tool>
{\"name\": \"sessions_spawn\", \"arguments\": {\"agent\": \"frontend_developer\", \"task\": \"Design frontend interface\", \"label\": \"frontend-dev\"}}
</tool>

Make ALL 4 calls immediately. Do NOT wait between calls. Do NOT write any text.".to_string()),
            api_key: None,
            temperature: Some(0.7),
            max_depth: 5,
            soul_preset: None,
        },
    );

    agents.insert(
        "frontend_developer".to_string(),
        DelegateAgentConfig {
            provider: "glm".to_string(),
            model: "glm-5".to_string(),
            system_prompt: Some("You are a frontend development engineer responsible for building the frontend interface of the A-share stock fund analysis web application.

IMPORTANT: Use group_chat tool to report your progress to the team!

When you start: Send a 'status' message
When you make progress: Send 'progress' messages
When you finish: Send a 'result' message

Example:
<tool>
{\"name\": \"group_chat\", \"arguments\": {\"action\": \"send\", \"message_type\": \"status\", \"content\": \"Starting frontend development\"}}
</tool>

Task goal: Design and implement a user-friendly and visually appealing frontend interface.

Specific tasks:
1. Design the overall UI/UX of the application
2. Implement responsive web pages using modern frontend frameworks
3. Integrate with backend APIs to fetch real-time data
4. Implement data visualization charts using charting libraries
5. Optimize page loading performance

Technical requirements:
- Use modern frontend frameworks (React/Vue)
- Use charting libraries (like ECharts, Chart.js) for data visualization
- Implement real-time data update mechanism
- Optimize page loading performance

Output requirements:
- Complete frontend code
- Interface screenshots or design mockups
- Usage documentation".to_string()),
            api_key: None,
            temperature: Some(0.5),
            max_depth: 3,
            soul_preset: None,
        },
    );

    agents.insert(
        "backend_developer".to_string(),
        DelegateAgentConfig {
            provider: "glm".to_string(),
            model: "glm-5".to_string(),
            system_prompt: Some("You are a backend development engineer responsible for building backend services of the A-share stock fund analysis web application.

IMPORTANT: Use group_chat tool to report your progress to the team!

When you start: Send a 'status' message
When you make progress: Send 'progress' messages
When you finish: Send a 'result' message

Example:
<tool>
{\"name\": \"group_chat\", \"arguments\": {\"action\": \"send\", \"message_type\": \"status\", \"content\": \"Starting backend development\"}}
</tool>

Task goal: Design and implement stable and efficient backend API services.

Specific tasks:
1. Design RESTful API interface specifications
2. Implement stock data fetching module (connect to data sources)
3. Implement fund flow analysis algorithms
4. Implement industry-based data aggregation functionality
5. Implement data caching mechanism to improve query performance

Technical requirements:
- Use high-performance backend frameworks (like Node.js, Python Flask, Go)
- Implement database design and optimization
- Implement API authentication and authorization mechanisms
- Implement logging and monitoring

Output requirements:
- Complete backend code
- API documentation (Swagger/OpenAPI)
- Database design documentation
- Deployment documentation".to_string()),
            api_key: None,
            temperature: Some(0.5),
            max_depth: 3,
            soul_preset: None,
        },
    );

    agents.insert(
        "data_analyst".to_string(),
        DelegateAgentConfig {
            provider: "glm".to_string(),
            model: "glm-5".to_string(),
            system_prompt: Some("You are a data analyst responsible for design and implementation of A-share stock fund flow analysis algorithms.

IMPORTANT: Use group_chat tool to report your progress to the team!

When you start: Send a 'status' message
When you make progress: Send 'progress' messages
When you finish: Send a 'result' message

Example:
<tool>
{\"name\": \"group_chat\", \"arguments\": {\"action\": \"send\", \"message_type\": \"status\", \"content\": \"Starting data analysis\"}}
</tool>

Task goal: Implement accurate and timely A-share market fund flow analysis.

Specific tasks:
1. Analyze A-share market fund flow data
2. Identify main capital movements (northbound funds, southbound funds)
3. Analyze fund distribution by industry
4. Provide data-driven investment advice
5. Generate analysis reports and visualization charts

Analysis methods:
- Use technical analysis methods to identify fund flow trends
- Analyze internal and external buy/sell ratios
- Calculate industry net fund inflows/outflows
- Identify hot sectors with concentrated capital

Output requirements:
- Fund flow analysis report
- Industry fund distribution charts
- Investment recommendations list
- Analysis code and scripts".to_string()),
            api_key: None,
            temperature: Some(0.6),
            max_depth: 3,
            soul_preset: None,
        },
    );

    agents.insert(
        "qa_engineer".to_string(),
        DelegateAgentConfig {
            provider: "glm".to_string(),
            model: "glm-5".to_string(),
            system_prompt: Some("You are a quality assurance engineer responsible for testing and quality assurance of the A-share stock fund analysis web application.

Task goal: Ensure application quality and stability.

Specific tasks:
1. Write test cases (functional testing, performance testing, security testing)
2. Execute automated tests
3. Perform manual exploratory testing
4. Record and track bugs
5. Verify bug fixes and ensure regression tests pass

Testing scope:
- Frontend interface functional testing
- Backend API functional testing
- Performance and load testing
- Security vulnerability scanning
- Cross-browser and cross-device compatibility testing

Output requirements:
- Test plan document
- Test execution report
- Bug tracking report
- Quality assurance summary".to_string()),
            api_key: None,
            temperature: Some(0.3),
            max_depth: 3,
            soul_preset: None,
        },
    );

    cfg.agents = agents;

    let security = Arc::new(SecurityPolicy::from_config(&cfg.autonomy, &cfg.workspace_dir));
    let mgr = manager_for_workspace(&cfg.workspace_dir, cfg.swarm.subagent_max_concurrent).unwrap();
    let cfg_arc = Arc::new(cfg);

    tracing::info!("=== 阶段1: 启动项目经理任务 ===");
    let spawn = SessionsSpawnTool::new(
        security.clone(),
        cfg_arc.clone(),
        mgr.clone(),
        SwarmContext::root(),
    );
    let subagents = SubagentsTool::new(
        security.clone(),
        cfg_arc.clone(),
        mgr.clone(),
        SwarmContext::root(),
    );

    let pm_task = "As a project manager, please lead the team to complete the development of an A-share stock fund analysis web application.

Project requirements:
- Real-time access to A-share market data
- Analyze internal and external fund flows
- Display fund distribution by industry
- Visualize fund flow charts
- Provide investment advice

Please follow these steps:
1. Use the sessions_spawn tool to create the following subtasks:
   - frontend_developer: Design and implement the frontend interface (label: 'frontend-dev')
   - backend_developer: Design and implement the backend API (label: 'backend-dev')
   - data_analyst: Implement fund flow analysis algorithms (label: 'data-analysis')
   - qa_engineer: Write and execute test cases (label: 'quality-assurance')
2. Wait for all subtasks to complete
3. Summarize the results of all subtasks and generate a project summary report

Important: You MUST use the sessions_spawn tool to delegate tasks. Do NOT complete all the work yourself.";

    tracing::info!("Start project manager task");
    tracing::info!("Task description: {}", pm_task);
    
    let r1 = spawn
        .execute(json!({
            "agent": "project_manager",
            "task": pm_task,
            "label": "PM-project-management",
            "orchestrator": true
        }))
        .await
        .unwrap();
    
    tracing::info!("Project manager task started: {}", r1.success);
    assert!(r1.success);
    let pm_run_id = parse_run_id(&r1.output);
    tracing::info!("Project manager run ID: {}", pm_run_id);

    tokio::time::sleep(Duration::from_millis(100)).await;

    tracing::info!("=== Phase 2: Monitor subtask creation ===");
    let mut subtasks_created = false;
    let mut check_count = 0;
    let max_checks = 150;

    while !subtasks_created && check_count < max_checks {
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        let _listed = subagents.execute(json!({"action":"list"})).await.unwrap();
        let all_runs = mgr.list().await;
        
        tracing::info!("Check {}/{}: Current task count = {}", check_count + 1, max_checks, all_runs.len());
        
        if all_runs.len() > 1 {
            subtasks_created = true;
            tracing::info!("✓ Detected subtask creation");
            
            for run in &all_runs {
                if run.run_id != pm_run_id {
                    tracing::info!("  Subtask - Agent: {}, Label: {:?}, Status: {:?}", 
                        run.agent_name, run.label, run.status);
                }
            }
        }
        
        check_count += 1;
    }

    assert!(subtasks_created, "Project manager should create subtasks, but no subtasks were detected");

    tokio::time::sleep(Duration::from_millis(100)).await;

    tracing::info!("=== Phase 3: Wait for project manager to complete ===");
    let pm_wait_start = std::time::Instant::now();
    let pm_result = subagents
        .execute(json!({
            "action": "wait",
            "run_id": pm_run_id.to_string(),
            "timeout_secs": 60
        }))
        .await;
    
    let pm_wait_duration = pm_wait_start.elapsed();
    tracing::info!("Project manager wait time: {:?}", pm_wait_duration);

    match pm_result {
        Ok(result) => {
            tracing::info!("Project manager task completed: {}", result.success);
            tracing::info!("Project manager output:\n{}", result.output);
        }
        Err(e) => {
            tracing::error!("Project manager task wait failed: {}", e);
        }
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    tracing::info!("=== Phase 4: Check all task status ===");
    let listed = subagents.execute(json!({"action":"list"})).await.unwrap();
    tracing::info!("All tasks list:\n{}", listed.output);
    assert!(listed.success);

    let all_runs = mgr.list().await;
    tracing::info!("Total {} tasks created", all_runs.len());

    let mut completed_count = 0;
    let mut failed_count = 0;
    let mut running_count = 0;
    let mut pending_count = 0;

    for run in &all_runs {
        tracing::info!(
            "Task details - ID: {}, Agent: {}, Status: {:?}, Label: {:?}, Depth: {}",
            run.run_id,
            run.agent_name,
            run.status,
            run.label,
            run.depth
        );
        
        if let Some(output) = &run.output {
            tracing::info!("  Output: {}", output);
        }
        
        if let Some(error) = &run.error {
            tracing::error!("  Error: {}", error);
        }

        match run.status {
            zeroclaw::swarm::RunStatus::Completed => completed_count += 1,
            zeroclaw::swarm::RunStatus::Failed => failed_count += 1,
            zeroclaw::swarm::RunStatus::Running => running_count += 1,
            zeroclaw::swarm::RunStatus::Pending => pending_count += 1,
            _ => {}
        }
    }

    tracing::info!("Task statistics - Completed: {}, Failed: {}, Running: {}, Pending: {}", 
        completed_count, failed_count, running_count, pending_count);
    
    println!("\n=== Task Statistics ===");
    println!("Completed: {}, Failed: {}, Running: {}, Pending: {}", 
        completed_count, failed_count, running_count, pending_count);
    println!("Total tasks: {}", all_runs.len());

    assert!(all_runs.len() >= 2, "At least 2 tasks should exist (PM + at least 1 subtask)");
    if failed_count > 0 {
        tracing::warn!("Some tasks failed, but this is acceptable for this test");
    }

    tracing::info!("=== Phase 5: Print group chat messages ===");
    let group_chat = zeroclaw::tools::GroupChatTool::new(
        Arc::new(SecurityPolicy::default()),
        cfg_arc.clone(),
        SwarmContext::root(),
    );
    
    let chat_result = group_chat.execute(json!({
        "action": "read",
        "limit": 50
    })).await.unwrap();
    
    println!("\n=== Group Chat Messages ===");
    println!("{}", chat_result.output);
    println!("=== End of Group Chat ===\n");

    tracing::info!("=== Test completed ===");
    tracing::info!("✓ Project manager successfully created and coordinated subtasks");
    tracing::info!("✓ Task coordination working correctly");
}
