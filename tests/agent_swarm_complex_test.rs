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
async fn test_a_stock_fund_analysis_complex_workflow() {
    let tmp = TempDir::new().unwrap();
    let workspace_dir = tmp.path().join("workspace");
    tokio::fs::create_dir_all(&workspace_dir).await.unwrap();

    tracing::info!("=== 开始A股股票资金分析web应用复杂测试 ===");
    tracing::info!("工作目录: {:?}", workspace_dir);

    let mut cfg = Config::default();
    cfg.workspace_dir = workspace_dir.clone();
    cfg.default_provider = Some("delay".to_string());
    cfg.default_model = Some("delay:10".to_string());
    cfg.swarm = SwarmConfig {
        subagent_max_concurrent: 3,
        orchestrator_prompt: None,
    };

    let mut agents = HashMap::new();

    agents.insert(
        "project_manager".to_string(),
        DelegateAgentConfig {
            provider: "delay".to_string(),
            model: "delay:500".to_string(),
            system_prompt: Some("你是项目经理，负责协调团队构建A股股票资金分析web应用。你需要：1. 分析需求并分解任务 2. 分配任务给合适的团队成员 3. 跟踪项目进度 4. 协调团队沟通 5. 确保项目按时交付。请使用subagents工具来分配任务给团队成员。".to_string()),
            api_key: None,
            temperature: Some(0.7),
            max_depth: 3,
            soul_preset: None,
        },
    );

    agents.insert(
        "frontend_developer".to_string(),
        DelegateAgentConfig {
            provider: "delay".to_string(),
            model: "delay:800".to_string(),
            system_prompt: Some("你是前端开发工程师，负责构建A股股票资金分析web应用的前端界面。你需要：1. 设计用户友好的界面 2. 实现股票数据可视化 3. 实现资金流向图表 4. 实现分行业筛选功能 5. 确保响应式设计。使用shell工具执行前端构建命令。".to_string()),
            api_key: None,
            temperature: Some(0.5),
            max_depth: 2,
            soul_preset: None,
        },
    );

    agents.insert(
        "backend_developer".to_string(),
        DelegateAgentConfig {
            provider: "delay".to_string(),
            model: "delay:800".to_string(),
            system_prompt: Some("你是后端开发工程师，负责构建A股股票资金分析web应用的后端服务。你需要：1. 设计RESTful API 2. 实现股票数据获取 3. 实现资金流向分析算法 4. 实现分行业数据聚合 5. 实现数据缓存机制。使用shell工具执行后端构建命令。".to_string()),
            api_key: None,
            temperature: Some(0.5),
            max_depth: 2,
            soul_preset: None,
        },
    );

    agents.insert(
        "data_analyst".to_string(),
        DelegateAgentConfig {
            provider: "delay".to_string(),
            model: "delay:600".to_string(),
            system_prompt: Some("你是数据分析师，负责A股股票资金分析web应用的数据分析功能。你需要：1. 分析A股市场资金流向 2. 识别主力资金动向 3. 分行业分析资金分布 4. 提供投资建议 5. 生成分析报告。使用shell工具执行数据分析脚本。".to_string()),
            api_key: None,
            temperature: Some(0.6),
            max_depth: 2,
            soul_preset: None,
        },
    );

    agents.insert(
        "qa_engineer".to_string(),
        DelegateAgentConfig {
            provider: "delay".to_string(),
            model: "delay:400".to_string(),
            system_prompt: Some("你是QA工程师，负责A股股票资金分析web应用的测试工作。你需要：1. 编写测试用例 2. 执行功能测试 3. 执行性能测试 4. 记录和跟踪bug 5. 验证修复。使用shell工具执行测试命令。".to_string()),
            api_key: None,
            temperature: Some(0.3),
            max_depth: 2,
            soul_preset: None,
        },
    );

    cfg.agents = agents;

    let security = Arc::new(SecurityPolicy::from_config(&cfg.autonomy, &cfg.workspace_dir));
    let mgr = manager_for_workspace(&cfg.workspace_dir, cfg.swarm.subagent_max_concurrent).unwrap();
    let cfg_arc = Arc::new(cfg);

    tracing::info!("=== 阶段1: 项目经理分析需求并分解任务 ===");
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

    let pm_task = "协调团队构建一个分行业的自动分析A股股票内外盘买卖资金分析的web应用。需要包含以下功能：1. 实时获取A股市场数据 2. 分析内外盘资金流向 3. 按行业分类展示资金分布 4. 可视化展示资金流向图表 5. 提供投资建议。请使用subagents工具分配任务给团队成员。";

    tracing::info!("启动项目经理任务: {}", pm_task);
    let r1 = spawn
        .execute(json!({
            "agent": "project_manager",
            "task": pm_task,
            "label": "PM-需求分析",
            "orchestrator": true
        }))
        .await
        .unwrap();
    
    tracing::info!("项目经理任务已启动: {}", r1.success);
    assert!(r1.success);
    let pm_run_id = parse_run_id(&r1.output);
    tracing::info!("项目经理运行ID: {}", pm_run_id);

    tokio::time::sleep(Duration::from_millis(100)).await;

    tracing::info!("=== 阶段2: 等待项目经理完成任务 ===");
    let pm_wait_start = std::time::Instant::now();
    let pm_result = subagents
        .execute(json!({
            "action": "wait",
            "run_id": pm_run_id.to_string(),
            "timeout_secs": 30
        }))
        .await;
    
    let pm_wait_duration = pm_wait_start.elapsed();
    tracing::info!("项目经理等待时间: {:?}", pm_wait_duration);

    match pm_result {
        Ok(result) => {
            tracing::info!("项目经理任务完成: {}", result.success);
            tracing::info!("项目经理输出: {}", result.output);
        }
        Err(e) => {
            tracing::error!("项目经理任务等待失败: {}", e);
        }
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    tracing::info!("=== 阶段3: 检查所有子任务状态 ===");
    let listed = subagents.execute(json!({"action":"list"})).await.unwrap();
    tracing::info!("所有任务列表:\n{}", listed.output);
    assert!(listed.success);

    let all_runs = mgr.list().await;
    tracing::info!("总共创建了 {} 个任务", all_runs.len());

    for run in &all_runs {
        tracing::info!(
            "任务详情 - ID: {}, Agent: {}, 状态: {:?}, 标签: {:?}, 深度: {}",
            run.run_id,
            run.agent_name,
            run.status,
            run.label,
            run.depth
        );
        
        if let Some(output) = &run.output {
            tracing::info!("  输出: {}", output);
        }
        
        if let Some(error) = &run.error {
            tracing::error!("  错误: {}", error);
        }
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    tracing::info!("=== 阶段4: 分析任务执行情况 ===");
    let mut completed_count = 0;
    let mut failed_count = 0;
    let mut running_count = 0;
    let mut pending_count = 0;

    for run in &all_runs {
        match run.status {
            zeroclaw::swarm::RunStatus::Completed => completed_count += 1,
            zeroclaw::swarm::RunStatus::Failed => failed_count += 1,
            zeroclaw::swarm::RunStatus::Running => running_count += 1,
            zeroclaw::swarm::RunStatus::Pending => pending_count += 1,
            _ => {}
        }
    }

    tracing::info!("任务统计:");
    tracing::info!("  已完成: {}", completed_count);
    tracing::info!("  失败: {}", failed_count);
    tracing::info!("  运行中: {}", running_count);
    tracing::info!("  等待中: {}", pending_count);

    tokio::time::sleep(Duration::from_millis(500)).await;

    tracing::info!("=== 阶段5: 等待所有子任务完成 ===");
    let mut all_completed = false;
    let mut wait_iterations = 0;
    let max_wait_iterations = 60;

    while !all_completed && wait_iterations < max_wait_iterations {
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        let current_runs = mgr.list().await;
        let mut still_running = false;
        
        for run in &current_runs {
            if !run.is_terminal() {
                still_running = true;
                tracing::info!(
                    "等待中 - 任务: {}, Agent: {}, 状态: {:?}",
                    run.run_id,
                    run.agent_name,
                    run.status
                );
            }
        }
        
        all_completed = !still_running;
        wait_iterations += 1;
        
        if wait_iterations % 10 == 0 {
            tracing::info!("已等待 {} 秒，仍有任务在运行...", wait_iterations);
        }
    }

    if all_completed {
        tracing::info!("所有任务已完成");
    } else {
        tracing::warn!("等待超时，部分任务可能未完成");
    }

    tracing::info!("=== 阶段6: 最终状态检查 ===");
    let final_runs = mgr.list().await;
    
    for run in &final_runs {
        tracing::info!(
            "最终状态 - ID: {}, Agent: {}, 状态: {:?}, 标签: {:?}",
            run.run_id,
            run.agent_name,
            run.status,
            run.label
        );
        
        if let Some(output) = &run.output {
            if output.len() > 500 {
                tracing::info!("  输出 (前500字符): {}...", &output[..500]);
            } else {
                tracing::info!("  输出: {}", output);
            }
        }
        
        if let Some(error) = &run.error {
            tracing::error!("  错误: {}", error);
        }
    }

    tracing::info!("=== 测试完成 ===");
    
    assert!(final_runs.len() > 0, "应该至少创建一个任务");
    
    let pm_final_run = final_runs.iter().find(|r| r.run_id == pm_run_id);
    assert!(pm_final_run.is_some(), "应该找到项目经理任务");
    
    let pm_run = pm_final_run.unwrap();
    tracing::info!("项目经理最终状态: {:?}", pm_run.status);
}

#[tokio::test]
async fn test_a_stock_fund_analysis_with_debug_logs() {
    let tmp = TempDir::new().unwrap();
    let workspace_dir = tmp.path().join("workspace_debug");
    tokio::fs::create_dir_all(&workspace_dir).await.unwrap();

    tracing::info!("=== 启动详细debug日志测试 ===");
    tracing::info!("工作目录: {:?}", workspace_dir);

    let mut cfg = Config::default();
    cfg.workspace_dir = workspace_dir.clone();
    cfg.default_provider = Some("delay".to_string());
    cfg.default_model = Some("delay:10".to_string());
    cfg.swarm = SwarmConfig {
        subagent_max_concurrent: 2,
        orchestrator_prompt: None,
    };

    let mut agents = HashMap::new();

    agents.insert(
        "orchestrator".to_string(),
        DelegateAgentConfig {
            provider: "delay".to_string(),
            model: "delay:300".to_string(),
            system_prompt: Some("你是协调器，负责分配A股股票资金分析任务。你需要：1. 将任务分解为子任务 2. 使用subagents工具分配给专业agent 3. 跟踪每个子任务的状态 4. 汇总结果。重要：你必须使用subagents工具来分配任务，不要自己完成所有工作。".to_string()),
            api_key: None,
            temperature: Some(0.7),
            max_depth: 2,
            soul_preset: None,
        },
    );

    agents.insert(
        "worker".to_string(),
        DelegateAgentConfig {
            provider: "delay".to_string(),
            model: "delay:500".to_string(),
            system_prompt: Some("你是工作agent，负责执行具体的A股股票资金分析任务。你需要：1. 理解分配给你的具体任务 2. 使用适当的工具完成任务 3. 返回清晰的结果。可用的工具包括：shell（执行命令）、file_read（读取文件）、file_write（写入文件）。".to_string()),
            api_key: None,
            temperature: Some(0.5),
            max_depth: 1,
            soul_preset: None,
        },
    );

    cfg.agents = agents;

    let security = Arc::new(SecurityPolicy::from_config(&cfg.autonomy, &cfg.workspace_dir));
    let mgr = manager_for_workspace(&cfg.workspace_dir, cfg.swarm.subagent_max_concurrent).unwrap();
    let cfg_arc = Arc::new(cfg);

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

    let task = "分析A股市场某行业的资金流向。请分解为以下子任务：1. 获取该行业股票列表 2. 分析每只股票的资金流向 3. 汇总行业整体资金流向 4. 生成分析报告。使用subagents工具分配这些子任务。";

    tracing::info!("步骤1: 启动协调器任务");
    tracing::info!("任务描述: {}", task);

    let start_time = std::time::Instant::now();
    
    let r1 = spawn
        .execute(json!({
            "agent": "orchestrator",
            "task": task,
            "label": "协调-资金分析",
            "orchestrator": true
        }))
        .await
        .unwrap();
    
    tracing::info!("协调器任务启动成功: {}", r1.success);
    let orchestrator_run_id = parse_run_id(&r1.output);
    tracing::info!("协调器运行ID: {}", orchestrator_run_id);
    tracing::info!("启动耗时: {:?}", start_time.elapsed());

    tokio::time::sleep(Duration::from_millis(200)).await;

    tracing::info!("步骤2: 检查协调器状态");
    let orchestrator_status = mgr.get(orchestrator_run_id).await;
    if let Some(run) = orchestrator_status {
        tracing::info!("协调器状态: {:?}", run.status);
        tracing::info!("协调器深度: {}", run.depth);
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    tracing::info!("步骤3: 列出所有任务");
    let all_runs = mgr.list().await;
    tracing::info!("当前任务数量: {}", all_runs.len());

    for (i, run) in all_runs.iter().enumerate() {
        tracing::info!(
            "任务[{}] - ID: {}, Agent: {}, 状态: {:?}, 父任务: {:?}",
            i + 1,
            run.run_id,
            run.agent_name,
            run.status,
            run.parent_run_id
        );
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    tracing::info!("步骤4: 等待协调器完成");
    let wait_start = std::time::Instant::now();
    
    let wait_result = subagents
        .execute(json!({
            "action": "wait",
            "run_id": orchestrator_run_id.to_string(),
            "timeout_secs": 20
        }))
        .await;
    
    tracing::info!("等待耗时: {:?}", wait_start.elapsed());

    match wait_result {
        Ok(result) => {
            tracing::info!("协调器任务完成: {}", result.success);
            if result.success {
                tracing::info!("协调器输出:\n{}", result.output);
            }
        }
        Err(e) => {
            tracing::error!("协调器任务等待出错: {}", e);
        }
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    tracing::info!("步骤5: 检查所有子任务");
    let final_runs = mgr.list().await;
    tracing::info!("最终任务数量: {}", final_runs.len());

    let mut orchestrator_children = Vec::new();
    for run in &final_runs {
        if let Some(parent) = run.parent_run_id {
            if parent == orchestrator_run_id {
                orchestrator_children.push(run.clone());
            }
        }
    }

    tracing::info!("协调器创建了 {} 个子任务", orchestrator_children.len());

    for (i, child) in orchestrator_children.iter().enumerate() {
        tracing::info!(
            "子任务[{}] - ID: {}, Agent: {}, 状态: {:?}, 标签: {:?}",
            i + 1,
            child.run_id,
            child.agent_name,
            child.status,
            child.label
        );

        if let Some(output) = &child.output {
            if output.len() > 300 {
                tracing::info!("  输出 (前300字符): {}...", &output[..300]);
            } else {
                tracing::info!("  输出: {}", output);
            }
        }

        if let Some(error) = &child.error {
            tracing::error!("  错误: {}", error);
        }
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    tracing::info!("步骤6: 等待所有子任务完成");
    let mut all_children_completed = false;
    let mut child_wait_iterations = 0;
    let max_child_wait = 30;

    while !all_children_completed && child_wait_iterations < max_child_wait {
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        let mut current_children = Vec::new();
        for child in &orchestrator_children {
            if let Some(run) = mgr.get(child.run_id).await {
                current_children.push(run);
            }
        }
        
        let still_running = current_children.iter().any(|r: &zeroclaw::swarm::SubagentRun| !r.is_terminal());
        
        if still_running {
            for child in &current_children {
                if !child.is_terminal() {
                    tracing::info!(
                        "子任务仍在运行 - ID: {}, Agent: {}, 状态: {:?}",
                        child.run_id,
                        child.agent_name,
                        child.status
                    );
                }
            }
        } else {
            all_children_completed = true;
            tracing::info!("所有子任务已完成");
        }
        
        child_wait_iterations += 1;
        
        if child_wait_iterations % 5 == 0 {
            tracing::info!("已等待子任务 {} 秒", child_wait_iterations);
        }
    }

    if !all_children_completed {
        tracing::warn!("子任务等待超时");
    }

    tracing::info!("步骤7: 最终状态汇总");
    let mut final_children = Vec::new();
    for child in &orchestrator_children {
        if let Some(run) = mgr.get(child.run_id).await {
            final_children.push(run);
        }
    }

    let mut completed = 0;
    let mut failed = 0;
    let mut running = 0;
    let mut pending = 0;

    for child in &final_children {
        match child.status {
            zeroclaw::swarm::RunStatus::Completed => completed += 1,
            zeroclaw::swarm::RunStatus::Failed => failed += 1,
            zeroclaw::swarm::RunStatus::Running => running += 1,
            zeroclaw::swarm::RunStatus::Pending => pending += 1,
            _ => {}
        }
    }

    tracing::info!("子任务统计:");
    tracing::info!("  已完成: {}", completed);
    tracing::info!("  失败: {}", failed);
    tracing::info!("  运行中: {}", running);
    tracing::info!("  等待中: {}", pending);

    let orchestrator_final = mgr.get(orchestrator_run_id).await;
    if let Some(run) = orchestrator_final {
        tracing::info!("协调器最终状态: {:?}", run.status);
        if let Some(output) = &run.output {
            tracing::info!("协调器最终输出:\n{}", output);
        }
    }

    tracing::info!("=== Debug日志测试完成 ===");
    tracing::info!("总耗时: {:?}", start_time.elapsed());

    assert!(final_runs.len() > 0, "应该至少创建一个任务");
}

#[tokio::test]
async fn test_a_stock_fund_analysis_communication_flow() {
    let tmp = TempDir::new().unwrap();
    let workspace_dir = tmp.path().join("workspace_comm");
    tokio::fs::create_dir_all(&workspace_dir).await.unwrap();

    tracing::info!("=== 测试agent间通信流程 ===");

    let mut cfg = Config::default();
    cfg.workspace_dir = workspace_dir.clone();
    cfg.default_provider = Some("delay".to_string());
    cfg.default_model = Some("delay:10".to_string());
    cfg.swarm = SwarmConfig {
        subagent_max_concurrent: 2,
        orchestrator_prompt: None,
    };

    let mut agents = HashMap::new();

    agents.insert(
        "leader".to_string(),
        DelegateAgentConfig {
            provider: "delay".to_string(),
            model: "delay:400".to_string(),
            system_prompt: Some("你是团队领导，负责协调A股股票资金分析项目。你需要：1. 分配任务给团队成员 2. 使用subagents工具与团队成员沟通 3. 跟踪任务进度 4. 汇总结果。重要：你必须使用subagents工具来分配任务，不要自己完成所有工作。".to_string()),
            api_key: None,
            temperature: Some(0.7),
            max_depth: 2,
            soul_preset: None,
        },
    );

    agents.insert(
        "member".to_string(),
        DelegateAgentConfig {
            provider: "delay".to_string(),
            model: "delay:600".to_string(),
            system_prompt: Some("你是团队成员，负责执行A股股票资金分析的具体任务。你需要：1. 理解分配给你的任务 2. 完成任务并返回结果 3. 如果需要更多信息，通过输出请求。".to_string()),
            api_key: None,
            temperature: Some(0.5),
            max_depth: 1,
            soul_preset: None,
        },
    );

    cfg.agents = agents;

    let security = Arc::new(SecurityPolicy::from_config(&cfg.autonomy, &cfg.workspace_dir));
    let mgr = manager_for_workspace(&cfg.workspace_dir, cfg.swarm.subagent_max_concurrent).unwrap();
    let cfg_arc = Arc::new(cfg);

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

    let task = "分析A股科技行业的资金流向。请分配任务给团队成员完成以下工作：1. 获取科技行业股票列表 2. 分析资金流向 3. 生成报告。使用subagents工具分配任务。";

    tracing::info!("步骤1: 启动团队领导");
    let r1 = spawn
        .execute(json!({
            "agent": "leader",
            "task": task,
            "label": "领导-科技行业分析",
            "orchestrator": true
        }))
        .await
        .unwrap();
    
    assert!(r1.success);
    let leader_run_id = parse_run_id(&r1.output);
    tracing::info!("团队领导运行ID: {}", leader_run_id);

    tokio::time::sleep(Duration::from_millis(300)).await;

    tracing::info!("步骤2: 检查是否创建了子任务");
    let runs_after_spawn = mgr.list().await;
    tracing::info!("启动后任务数量: {}", runs_after_spawn.len());

    let mut child_tasks = Vec::new();
    for run in &runs_after_spawn {
        if let Some(parent) = run.parent_run_id {
            if parent == leader_run_id {
                child_tasks.push(run.clone());
                tracing::info!(
                    "发现子任务 - ID: {}, Agent: {}, 状态: {:?}",
                    run.run_id,
                    run.agent_name,
                    run.status
                );
            }
        }
    }

    if child_tasks.is_empty() {
        tracing::warn!("警告：团队领导没有创建子任务");
        tracing::info!("这可能是因为：");
        tracing::info!("1. 团队领导没有使用subagents工具");
        tracing::info!("2. 团队领导的系统提示不够明确");
        tracing::info!("3. 任务描述不够清晰");
    } else {
        tracing::info!("团队领导成功创建了 {} 个子任务", child_tasks.len());
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    tracing::info!("步骤3: 等待团队领导完成");
    let leader_result = subagents
        .execute(json!({
            "action": "wait",
            "run_id": leader_run_id.to_string(),
            "timeout_secs": 15
        }))
        .await;

    match leader_result {
        Ok(result) => {
            tracing::info!("团队领导任务完成: {}", result.success);
            if result.success {
                tracing::info!("团队领导输出:\n{}", result.output);
            }
        }
        Err(e) => {
            tracing::error!("团队领导任务等待出错: {}", e);
        }
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    tracing::info!("步骤4: 最终状态检查");
    let final_runs = mgr.list().await;
    let leader_final = mgr.get(leader_run_id).await;

    if let Some(run) = leader_final {
        tracing::info!("团队领导最终状态: {:?}", run.status);
        
        if let Some(output) = &run.output {
            tracing::info!("团队领导输出:\n{}", output);
            
            if output.contains("subagents") || output.contains("spawn") {
                tracing::info!("团队领导尝试使用subagents工具");
            } else {
                tracing::warn!("团队领导可能没有使用subagents工具");
            }
        }
    }

    let final_children: Vec<_> = final_runs
        .iter()
        .filter(|r| r.parent_run_id == Some(leader_run_id))
        .collect();

    tracing::info!("最终子任务数量: {}", final_children.len());

    for (i, child) in final_children.iter().enumerate() {
        tracing::info!(
            "子任务[{}] - ID: {}, Agent: {}, 状态: {:?}",
            i + 1,
            child.run_id,
            child.agent_name,
            child.status
        );

        if let Some(output) = &child.output {
            tracing::info!("  输出: {}", output);
        }
    }

    tracing::info!("=== 通信流程测试完成 ===");

    assert!(final_runs.len() > 0, "应该至少创建一个任务");
}
