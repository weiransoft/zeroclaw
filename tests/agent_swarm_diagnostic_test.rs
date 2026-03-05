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
async fn diagnose_orchestrator_behavior() {
    let tmp = TempDir::new().unwrap();
    let workspace_dir = tmp.path().join("workspace_diagnose");
    tokio::fs::create_dir_all(&workspace_dir).await.unwrap();

    tracing::info!("=== 诊断测试：分析orchestrator行为 ===");
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
            system_prompt: Some("你是协调器，负责分配任务。你必须使用subagents工具来分配任务给worker agent。不要自己完成所有工作。步骤：1. 分析任务 2. 使用subagents工具的spawn action创建子任务 3. 等待子任务完成 4. 汇总结果。".to_string()),
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
            system_prompt: Some("你是工作agent，负责执行具体任务。完成分配给你的任务并返回结果。".to_string()),
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

    let task = "分析A股科技行业资金流向。请使用subagents工具分配任务给worker agent完成：1. 获取股票列表 2. 分析资金流向 3. 生成报告。";

    tracing::info!("步骤1: 启动orchestrator");
    tracing::info!("任务: {}", task);

    let start_time = std::time::Instant::now();
    
    let r1 = spawn
        .execute(json!({
            "agent": "orchestrator",
            "task": task,
            "label": "协调-任务分配",
            "orchestrator": true
        }))
        .await
        .unwrap();
    
    tracing::info!("orchestrator启动成功: {}", r1.success);
    let orchestrator_run_id = parse_run_id(&r1.output);
    tracing::info!("orchestrator运行ID: {}", orchestrator_run_id);

    tokio::time::sleep(Duration::from_millis(100)).await;

    tracing::info!("步骤2: 检查orchestrator状态");
    let orchestrator_status = mgr.get(orchestrator_run_id).await;
    if let Some(run) = orchestrator_status {
        tracing::info!("orchestrator状态: {:?}", run.status);
        tracing::info!("orchestrator深度: {}", run.depth);
        tracing::info!("orchestrator是否可生成子任务: {}", run.orchestrator);
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    tracing::info!("步骤3: 检查是否创建了子任务");
    let runs_after_200ms = mgr.list().await;
    tracing::info!("200ms后任务数量: {}", runs_after_200ms.len());

    let mut child_tasks = Vec::new();
    for run in &runs_after_200ms {
        if let Some(parent) = run.parent_run_id {
            if parent == orchestrator_run_id {
                child_tasks.push(run.clone());
                tracing::info!(
                    "发现子任务 - ID: {}, Agent: {}, 状态: {:?}, 创建时间: {}",
                    run.run_id,
                    run.agent_name,
                    run.status,
                    run.started_at_unix
                );
            }
        }
    }

    if child_tasks.is_empty() {
        tracing::warn!("警告：orchestrator在200ms内没有创建子任务");
        tracing::info!("可能的原因：");
        tracing::info!("1. orchestrator没有使用subagents工具");
        tracing::info!("2. orchestrator正在思考如何分配任务");
        tracing::info!("3. orchestrator的系统提示不够明确");
    } else {
        tracing::info!("orchestrator成功创建了 {} 个子任务", child_tasks.len());
    }

    tokio::time::sleep(Duration::from_millis(300)).await;

    tracing::info!("步骤4: 再次检查子任务");
    let runs_after_500ms = mgr.list().await;
    tracing::info!("500ms后任务数量: {}", runs_after_500ms.len());

    let mut child_tasks_500ms = Vec::new();
    for run in &runs_after_500ms {
        if let Some(parent) = run.parent_run_id {
            if parent == orchestrator_run_id {
                child_tasks_500ms.push(run.clone());
                tracing::info!(
                    "发现子任务 - ID: {}, Agent: {}, 状态: {:?}",
                    run.run_id,
                    run.agent_name,
                    run.status
                );
            }
        }
    }

    if child_tasks_500ms.len() > child_tasks.len() {
        tracing::info!("orchestrator在200ms-500ms之间创建了 {} 个新子任务", child_tasks_500ms.len() - child_tasks.len());
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    tracing::info!("步骤5: 检查orchestrator是否完成");
    let orchestrator_after_1s = mgr.get(orchestrator_run_id).await;
    if let Some(run) = orchestrator_after_1s {
        tracing::info!("orchestrator状态: {:?}", run.status);
        
        if run.is_terminal() {
            tracing::info!("orchestrator已完成");
            if let Some(output) = &run.output {
                tracing::info!("orchestrator输出:\n{}", output);
                
                if output.contains("subagents") || output.contains("spawn") {
                    tracing::info!("orchestrator尝试使用subagents工具");
                } else {
                    tracing::warn!("orchestrator可能没有使用subagents工具");
                }
            }
            if let Some(error) = &run.error {
                tracing::error!("orchestrator错误: {}", error);
            }
        } else {
            tracing::info!("orchestrator仍在运行中");
        }
    }

    tokio::time::sleep(Duration::from_secs(1)).await;

    tracing::info!("步骤6: 等待orchestrator完成");
    let wait_start = std::time::Instant::now();
    
    let wait_result = subagents
        .execute(json!({
            "action": "wait",
            "run_id": orchestrator_run_id.to_string(),
            "timeout_secs": 10
        }))
        .await;

    tracing::info!("等待耗时: {:?}", wait_start.elapsed());

    match wait_result {
        Ok(result) => {
            tracing::info!("orchestrator任务完成: {}", result.success);
            if result.success {
                tracing::info!("orchestrator输出:\n{}", result.output);
                
                if result.output.contains("subagents") || result.output.contains("spawn") {
                    tracing::info!("orchestrator在输出中提到了subagents工具");
                } else {
                    tracing::warn!("orchestrator的输出中没有提到subagents工具");
                }
            }
        }
        Err(e) => {
            tracing::error!("orchestrator任务等待出错: {}", e);
        }
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    tracing::info!("步骤7: 最终状态检查");
    let final_runs = mgr.list().await;
    let orchestrator_final = mgr.get(orchestrator_run_id).await;

    if let Some(ref run) = orchestrator_final {
        tracing::info!("orchestrator最终状态: {:?}", run.status);
        
        if let Some(output) = &run.output {
            tracing::info!("orchestrator最终输出:\n{}", output);
        }
        
        if let Some(error) = &run.error {
            tracing::error!("orchestrator错误: {}", error);
        }
    }

    let final_children: Vec<_> = final_runs
        .iter()
        .filter(|r| r.parent_run_id == Some(orchestrator_run_id))
        .collect();

    tracing::info!("最终子任务数量: {}", final_children.len());

    for (i, child) in final_children.iter().enumerate() {
        tracing::info!(
            "子任务[{}] - ID: {}, Agent: {}, 状态: {:?}, 标签: {:?}",
            i + 1,
            child.run_id,
            child.agent_name,
            child.status,
            child.label
        );

        if let Some(output) = &child.output {
            if output.len() > 200 {
                tracing::info!("  输出 (前200字符): {}...", &output[..200]);
            } else {
                tracing::info!("  输出: {}", output);
            }
        }

        if let Some(error) = &child.error {
            tracing::error!("  错误: {}", error);
        }
    }

    tracing::info!("=== 诊断总结 ===");
    tracing::info!("总耗时: {:?}", start_time.elapsed());
    tracing::info!("orchestrator最终状态: {:?}", orchestrator_final.map(|r| r.status));
    tracing::info!("子任务总数: {}", final_children.len());
    
    if final_children.is_empty() {
        tracing::warn!("orchestrator没有创建任何子任务");
        tracing::info!("可能的问题：");
        tracing::info!("1. orchestrator的系统提示不够明确，没有强调必须使用subagents工具");
        tracing::info!("2. orchestrator的任务描述不够清晰");
        tracing::info!("3. orchestrator的max_depth设置过低");
        tracing::info!("4. orchestrator没有正确理解需要分配任务");
    } else {
        tracing::info!("orchestrator成功创建了子任务");
    }

    assert!(final_runs.len() > 0, "应该至少创建一个任务");
}

#[tokio::test]
async fn diagnose_tool_usage_tracking() {
    let tmp = TempDir::new().unwrap();
    let workspace_dir = tmp.path().join("workspace_tool_tracking");
    tokio::fs::create_dir_all(&workspace_dir).await.unwrap();

    tracing::info!("=== 诊断测试：追踪工具使用 ===");

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
        "coordinator".to_string(),
        DelegateAgentConfig {
            provider: "delay".to_string(),
            model: "delay:300".to_string(),
            system_prompt: Some("你是协调器。你必须使用subagents工具来分配任务。步骤：1. 分析任务 2. 使用subagents工具的spawn action创建子任务，指定agent为'worker' 3. 等待子任务完成 4. 汇总结果。重要：你必须使用subagents工具，不要自己完成工作。".to_string()),
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
            system_prompt: Some("你是工作agent。完成分配给你的任务并返回结果。".to_string()),
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

    let task = "分配任务给worker agent完成简单工作：计算1+1的结果。使用subagents工具。";

    tracing::info!("步骤1: 启动coordinator");
    tracing::info!("任务: {}", task);

    let r1 = spawn
        .execute(json!({
            "agent": "coordinator",
            "task": task,
            "label": "协调-简单任务",
            "orchestrator": true
        }))
        .await
        .unwrap();
    
    assert!(r1.success);
    let coordinator_run_id = parse_run_id(&r1.output);
    tracing::info!("coordinator运行ID: {}", coordinator_run_id);

    tokio::time::sleep(Duration::from_millis(100)).await;

    tracing::info!("步骤2: 监控任务创建过程");
    let mut check_count = 0;
    let max_checks = 20;
    let mut child_created = false;
    let mut child_created_time = None;

    while check_count < max_checks && !child_created {
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let current_runs = mgr.list().await;
        let has_child = current_runs.iter().any(|r| r.parent_run_id == Some(coordinator_run_id));
        
        if has_child && !child_created {
            child_created = true;
            child_created_time = Some(std::time::Instant::now());
            tracing::info!("子任务在 {}ms 后创建", (check_count + 1) * 100);
            
            for run in &current_runs {
                if run.parent_run_id == Some(coordinator_run_id) {
                    tracing::info!(
                        "子任务详情 - ID: {}, Agent: {}, 状态: {:?}",
                        run.run_id,
                        run.agent_name,
                        run.status
                    );
                }
            }
        }
        
        check_count += 1;
    }

    if !child_created {
        tracing::warn!("在 {}ms 内没有检测到子任务创建", max_checks * 100);
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    tracing::info!("步骤3: 检查coordinator状态");
    let coordinator_status = mgr.get(coordinator_run_id).await;
    if let Some(run) = coordinator_status {
        tracing::info!("coordinator状态: {:?}", run.status);
        
        if run.is_terminal() {
            tracing::info!("coordinator已完成");
            if let Some(output) = &run.output {
                tracing::info!("coordinator输出:\n{}", output);
                
                let has_subagents = output.contains("subagents");
                let has_spawn = output.contains("spawn");
                let has_tool_call = output.contains("invoke") || output.contains("tool");
                
                tracing::info!("输出分析:");
                tracing::info!("  包含'subagents': {}", has_subagents);
                tracing::info!("  包含'spawn': {}", has_spawn);
                tracing::info!("  包含工具调用相关词: {}", has_tool_call);
                
                if !has_subagents && !has_spawn {
                    tracing::warn!("coordinator的输出中没有提到subagents或spawn");
                    tracing::info!("这表明coordinator可能没有使用subagents工具");
                }
            }
        } else {
            tracing::info!("coordinator仍在运行");
        }
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    tracing::info!("步骤4: 等待coordinator完成");
    let wait_result = subagents
        .execute(json!({
            "action": "wait",
            "run_id": coordinator_run_id.to_string(),
            "timeout_secs": 10
        }))
        .await;

    match wait_result {
        Ok(result) => {
            tracing::info!("coordinator任务完成: {}", result.success);
            if result.success {
                tracing::info!("coordinator输出:\n{}", result.output);
            }
        }
        Err(e) => {
            tracing::error!("coordinator任务等待出错: {}", e);
        }
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    tracing::info!("步骤5: 最终分析");
    let final_runs = mgr.list().await;
    let final_children: Vec<_> = final_runs
        .iter()
        .filter(|r| r.parent_run_id == Some(coordinator_run_id))
        .collect();

    tracing::info!("最终子任务数量: {}", final_children.len());

    for child in final_children {
        tracing::info!(
            "子任务 - ID: {}, Agent: {}, 状态: {:?}",
            child.run_id,
            child.agent_name,
            child.status
        );
    }

    tracing::info!("=== 工具使用追踪总结 ===");
    if let Some(creation_time) = child_created_time {
        tracing::info!("子任务创建时间: {:?}", creation_time.elapsed());
    } else {
        tracing::warn!("没有检测到子任务创建");
        tracing::info!("可能的原因：");
        tracing::info!("1. coordinator没有使用subagents工具");
        tracing::info!("2. coordinator的任务描述不够明确");
        tracing::info!("3. coordinator的系统提示需要改进");
    }

    assert!(final_runs.len() > 0, "应该至少创建一个任务");
}

#[tokio::test]
async fn diagnose_communication_delay() {
    let tmp = TempDir::new().unwrap();
    let workspace_dir = tmp.path().join("workspace_comm_delay");
    tokio::fs::create_dir_all(&workspace_dir).await.unwrap();

    tracing::info!("=== 诊断测试：分析通信延迟 ===");

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
        "manager".to_string(),
        DelegateAgentConfig {
            provider: "delay".to_string(),
            model: "delay:200".to_string(),
            system_prompt: Some("你是经理。你必须使用subagents工具分配任务给assistant。不要自己完成工作。".to_string()),
            api_key: None,
            temperature: Some(0.7),
            max_depth: 2,
            soul_preset: None,
        },
    );

    agents.insert(
        "assistant".to_string(),
        DelegateAgentConfig {
            provider: "delay".to_string(),
            model: "delay:300".to_string(),
            system_prompt: Some("你是助手。完成分配给你的任务。".to_string()),
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

    let task = "分配一个简单任务给assistant：输出'Hello World'。使用subagents工具。";

    tracing::info!("步骤1: 启动manager");
    let r1 = spawn
        .execute(json!({
            "agent": "manager",
            "task": task,
            "label": "经理-任务分配",
            "orchestrator": true
        }))
        .await
        .unwrap();
    
    assert!(r1.success);
    let manager_run_id = parse_run_id(&r1.output);
    tracing::info!("manager运行ID: {}", manager_run_id);

    tracing::info!("步骤2: 详细监控时间线");
    let mut timeline = Vec::new();
    let start_time = std::time::Instant::now();
    
    for i in 0..30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let runs = mgr.list().await;
        let manager = runs.iter().find(|r| r.run_id == manager_run_id);
        let children: Vec<_> = runs.iter().filter(|r| r.parent_run_id == Some(manager_run_id)).collect();
        
        timeline.push((i * 100, manager.map(|m| m.status.clone()), children.len()));
        
        if children.len() > 0 && timeline.len() > 1 {
            let prev_children = timeline[timeline.len() - 2].2;
            if children.len() > prev_children {
                tracing::info!("{}ms: 第一个子任务创建", i * 100);
            }
        }
        
        if let Some(m) = manager {
            if m.is_terminal() {
                tracing::info!("{}ms: manager完成 (状态: {:?})", i * 100, m.status);
                break;
            }
        }
    }

    tracing::info!("步骤3: 时间线分析");
    for (time, status, child_count) in &timeline {
        tracing::info!("{}ms - manager状态: {:?}, 子任务数: {}", time, status, child_count);
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    tracing::info!("步骤4: 最终状态");
    let final_runs = mgr.list().await;
    let final_children: Vec<_> = final_runs
        .iter()
        .filter(|r| r.parent_run_id == Some(manager_run_id))
        .collect();

    tracing::info!("最终子任务数量: {}", final_children.len());
    tracing::info!("总耗时: {:?}", start_time.elapsed());

    if final_children.is_empty() {
        tracing::warn!("manager没有创建子任务");
        tracing::info!("建议：");
        tracing::info!("1. 检查manager的系统提示是否明确要求使用subagents工具");
        tracing::info!("2. 检查任务描述是否清晰");
        tracing::info!("3. 考虑增加max_depth以允许更多嵌套");
    }

    assert!(final_runs.len() > 0, "应该至少创建一个任务");
}
