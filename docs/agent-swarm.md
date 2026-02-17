# Agent Swarm — 设计与落地蓝图（ZeroClaw）

本文档描述 ZeroClaw 中"智能体群组（agent swarm）"的完整实现，包含 spawn、运行注册表、lane 并发队列、wait/kill/steer、结果回传约束，以及群聊通信、共识机制和任务依赖管理。

## 1. 目标与非目标

### 1.1 目标

- 让主 agent 能以"群组/编排"的方式把一个任务拆为多个子任务并并发执行，然后聚合结果输出
- 支持关键闭环：
  - spawn 子 agent run（带 label、深度、可选 orchestrator/leaf 约束）
  - registry 记录 run 生命周期与元数据，并支持查询/等待
  - lane 队列与并发控制（main/subagent/nested 的概念）
  - kill/steer：终止子 run 或对其输入进行纠偏并重新运行
  - 群聊通信：智能体之间的消息传递和讨论
  - 共识机制：团队决策和冲突解决
  - 任务依赖管理：任务之间的依赖关系和协调
- 严格遵循 ZeroClaw 架构原则：trait + 工厂为扩展点，避免跨子系统耦合；安全默认最小权限；失败显式

### 1.2 非目标

- 不实现跨进程/跨机器的 swarm 分布式调度（本设计聚焦单进程内的 lane 队列与任务编排）
- 不实现"异步推送到 LLM history 的实时 announce queue"（使用显式 wait/list 拉取与可选的 memory 记录作为闭环）

## 2. 总体架构

### 2.1 核心组件

#### SwarmManager
- 职责：管理子 agent 的生命周期、注册表和队列调度
- 功能：
  - spawn：创建并启动子 agent run
  - list：列出所有子 agent run
  - get：获取特定 run 的详细信息
  - wait：等待 run 完成（支持超时）
  - kill：终止 run（级联终止子孙）
  - steer：纠偏并重新运行
- 持久化：通过 SwarmSqliteStore 存储到 SQLite 数据库

#### LaneQueue
- 职责：按 lane 控制并发与 FIFO 排队
- 功能：
  - enqueue：提交任务到指定 lane
  - cancel_pending：取消队列中的待执行任务
  - abort_running：中止正在运行的任务
- lane 类型：
  - `subagent`：子代理并发池
  - 可扩展：`main`、`nested` 等

#### SwarmSqliteStore
- 职责：提供 SQLite 持久化存储
- 功能：
  - 子 agent run 的增删改查
  - 事件日志记录
  - 群聊消息存储
  - 共识数据存储
  - 任务依赖存储
  - 进度和跟踪数据存储
- 数据库路径：`<workspace_dir>/.zeroclaw/swarm.db`

#### SwarmChatManager
- 职责：管理智能体之间的群聊通信
- 功能：
  - send_message：发送消息
  - send_task_assignment：发送任务分配
  - send_task_status：发送任务状态更新
  - request_consensus：发起共识请求
  - respond_consensus：响应共识请求
  - report_disagreement：报告分歧
  - request_clarification：请求澄清
  - respond_clarification：响应澄清

#### ConsensusManager
- 职责：管理团队共识机制
- 功能：
  - initiate_consensus：发起共识
  - vote：投票
  - report_disagreement：报告分歧
  - request_clarification：请求澄清
  - respond_clarification：响应澄清
  - check_consensus：检查是否达成共识

#### TaskDependencyManager
- 职责：管理任务之间的依赖关系
- 功能：
  - define_dependency：定义任务依赖
  - analyze_dependencies：分析依赖关系
  - get_ready_tasks：获取可执行的任务
  - update_task_status：更新任务状态
  - calculate_critical_path：计算关键路径
  - get_coordination_requests：获取协调请求

### 2.2 数据流

1) 主 agent（或用户）触发 `sessions_spawn`（JSON 参数包含：agent、task、label、requester、cleanup、orchestrator、max_depth 等）

2) `sessions_spawn` 经过 `SecurityPolicy` 校验后：
   - 在 registry 创建 run 记录（status=Pending）
   - 把执行闭包 enqueue 到 lane=`subagent`

3) lane 队列开始执行：
   - 基于 Config.agents 中的子 agent 配置（provider/model/temp/system_prompt）
   - 构造子 agent 系统提示词
   - 复用 ZeroClaw 现有的 `agent::process_message` 逻辑
   - 确保工具调用与安全策略一致

4) 子 run 完成后：
   - registry 更新 status=Completed/Failed
   - 持久化结果到 SQLite 数据库
   - 发送群聊消息通知相关智能体

5) 主 agent 通过 `subagents wait` 或 `subagents list` 拉取结果并进行聚合输出

### 2.3 群聊通信流程

1) 智能体通过 SwarmChatManager 发送消息
2) 消息存储在 SQLite 数据库的 swarm_chat_extended 表
3) 其他智能体可以查询和响应消息
4) 支持的消息类型：
   - TaskAssignment：任务分配
   - TaskStatus：任务状态
   - TaskProgress：任务进度
   - TaskCompletion：任务完成
   - TaskFailure：任务失败
   - ConsensusRequest：共识请求
   - ConsensusResponse：共识响应
   - Disagreement：分歧报告
   - Clarification：澄清请求/响应
   - Correction：纠正
   - Info：信息

### 2.4 共识机制流程

1) 智能体发起共识请求（ConsensusManager.initiate_consensus）
2) 参与智能体收到请求并投票（ConsensusManager.vote）
3) 如果有分歧，智能体可以报告分歧（ConsensusManager.report_disagreement）
4) 可以请求澄清（ConsensusManager.request_clarification）
5) 最终达成共识或失败

### 2.5 任务依赖协调流程

1) 定义任务依赖关系（TaskDependencyManager.define_dependency）
2) 分析依赖关系（TaskDependencyManager.analyze_dependencies）
3) 获取可执行任务（TaskDependencyManager.get_ready_tasks）
4) 任务完成后更新状态（TaskDependencyManager.update_task_status）
5) 计算关键路径（TaskDependencyManager.calculate_critical_path）

## 3. 子 agent 系统提示词（约束）

目标：把"只做任务、避免跑偏、避免主动外联"的规则固化为系统级约束

提示词结构：

- 身份：你是 ZeroClaw 的子代理，任务是 `<label>`
- 输入来源：requester（可选，字符串，不写入隐私/密钥）
- 深度与编排：如果 `orchestrator=true`，允许 spawn 子代理；否则禁止 spawn（防止无限递归）
- 输出要求：必须产出可被上层汇总的结构化结果（建议：标题 + 要点 + 风险/依赖）
- 语言支持：支持中文和英文通信

## 4. 并发与 lane 设计

### 4.1 lane

- `subagent`：子代理并发池
- 可扩展：`main`、`nested` 等

### 4.2 LaneQueue 行为

- 每个 lane 一个 `Semaphore` 控制最大并发
- 每个 lane 一个 FIFO 队列（`VecDeque`），由单独 pump task 负责取任务并在 `Semaphore` permit 下执行
- 任务执行返回 `oneshot` 结果
- kill/abort：
  - 对"未开始的任务"从队列中移除并标记为 Cancelled
  - 对"正在执行的任务"通过 `AbortHandle` 终止 tokio task，并标记为 Terminated

## 5. registry 与持久化

### 5.1 Run 数据模型（核心字段）

- `run_id`：UUID
- `parent_run_id`：可选
- `agent_name`：来自 `Config.agents` 的 key
- `label`：可选
- `task`：子任务文本
- `status`：Pending/Running/Completed/Failed/Cancelled/Terminated
- `started_at_unix`/`ended_at_unix`
- `output`：完成输出（可选）
- `error`：失败原因（可选）
- `depth`：spawn 深度
- `children`：子 run 列表（用于级联 kill）
- `cleanup`：是否在完成后自动清理
- `orchestrator`：是否允许再 spawn
- `owner_instance`：所属实例 ID
- `last_heartbeat_unix`：最后心跳时间

### 5.2 持久化位置

- 数据库：`<workspace_dir>/.zeroclaw/swarm.db`
- 数据库类型：SQLite（支持 WAL 模式、并发访问）
- 写入策略：每次状态变化后立即写入（通过 SQLite 事务保证原子性）
- 读取策略：启动时懒加载（首次使用 swarm 时加载）
- 迁移支持：从旧的 JSON 文件（subagents.json）自动迁移到 SQLite

### 5.3 数据库 Schema

#### subagent_runs 表
```sql
CREATE TABLE subagent_runs (
    run_id              TEXT PRIMARY KEY,
    parent_run_id       TEXT,
    agent_name          TEXT NOT NULL,
    label               TEXT,
    task                TEXT NOT NULL,
    orchestrator        INTEGER NOT NULL DEFAULT 0,
    status              TEXT NOT NULL,
    depth               INTEGER NOT NULL,
    started_at_unix     INTEGER NOT NULL,
    ended_at_unix       INTEGER,
    output              TEXT,
    error               TEXT,
    children_json       TEXT NOT NULL DEFAULT '[]',
    cleanup             INTEGER NOT NULL DEFAULT 0,
    owner_instance      TEXT NOT NULL,
    last_heartbeat_unix INTEGER
);
```

#### swarm_events 表
```sql
CREATE TABLE swarm_events (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    ts_unix INTEGER NOT NULL,
    run_id  TEXT,
    kind    TEXT NOT NULL,
    payload TEXT NOT NULL
);
```

#### swarm_chat_extended 表
```sql
CREATE TABLE swarm_chat_extended (
    id          TEXT PRIMARY KEY,
    ts_unix     INTEGER NOT NULL,
    run_id      TEXT,
    task_id     TEXT,
    author      TEXT NOT NULL,
    author_type TEXT NOT NULL,
    message_type TEXT NOT NULL,
    lang        TEXT NOT NULL,
    content     TEXT NOT NULL,
    parent_id   TEXT,
    metadata    TEXT NOT NULL DEFAULT '{}'
);
```

#### task_consensus 表
```sql
CREATE TABLE task_consensus (
    task_id           TEXT PRIMARY KEY,
    run_id            TEXT,
    status            TEXT NOT NULL,
    topic             TEXT NOT NULL,
    participants_json  TEXT NOT NULL DEFAULT '[]',
    votes_json        TEXT NOT NULL DEFAULT '{}',
    disagreements_json TEXT NOT NULL DEFAULT '[]',
    clarifications_json TEXT NOT NULL DEFAULT '[]',
    resolution        TEXT,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL
);
```

#### task_dependencies 表
```sql
CREATE TABLE task_dependencies (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id             TEXT NOT NULL,
    depends_on          TEXT NOT NULL,
    dependency_type     TEXT NOT NULL,
    condition           TEXT,
    required_data_json  TEXT,
    required_resources_json TEXT,
    status              TEXT NOT NULL,
    blocking_reason     TEXT,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL
);
```

#### progress_entries 表
```sql
CREATE TABLE progress_entries (
    id              TEXT PRIMARY KEY,
    run_id          TEXT,
    task_id         TEXT,
    status          TEXT NOT NULL,
    title           TEXT NOT NULL,
    description     TEXT,
    progress        REAL NOT NULL DEFAULT 0.0,
    total           REAL,
    unit            TEXT,
    started_at      INTEGER,
    updated_at      INTEGER NOT NULL,
    completed_at    INTEGER,
    error           TEXT,
    metadata        TEXT NOT NULL DEFAULT '{}'
);
```

#### trace_entries 表
```sql
CREATE TABLE trace_entries (
    id          TEXT PRIMARY KEY,
    run_id      TEXT,
    task_id     TEXT,
    parent_id   TEXT,
    timestamp   INTEGER NOT NULL,
    level       TEXT NOT NULL,
    message     TEXT NOT NULL,
    lang        TEXT NOT NULL DEFAULT 'en',
    metadata    TEXT NOT NULL DEFAULT '{}'
);
```

## 6. Tool 接口设计

### 6.1 sessions_spawn

参数（JSON）：

- `agent`（必填）：delegate agent 名称（对应 `config.agents` key）
- `task`（必填）：子任务文本
- `label`（可选）：用于上层聚合与展示
- `orchestrator`（可选，默认 false）：是否允许该子代理再 spawn
- `parent_run_id`（可选）：用于建立父子关系（上层可自行传入）
- `cleanup`（可选，默认 false）：是否在完成后自动清理 registry 记录

返回：

- `run_id`
- `status`（running）
- `hint`：提示上层用 `subagents wait` 拉取结果

### 6.2 subagents

参数（JSON）：

- `action`：`list|get|wait|kill|steer`
- `run_id`：除 list 外必填
- `timeout_secs`：仅用于 wait（0 = 无超时）
- `message`：steer 时必填（作为新的 task/纠偏输入）

返回：

- list：run 摘要列表（id/agent/status/label/age）
- get/wait：包含 output/error 的详细信息
- kill：确认结果（是否终止/取消成功）
- steer：新 run_id（替换旧 run 的策略：旧 run 标记 Terminated，新 run 继承 parent/label）

## 7. 安全策略与限制

- `sessions_spawn`、`kill`、`steer` 都属于"会触发副作用的动作"，必须：
  - `security.can_act()` 为 true
  - `security.record_action()` 成功（速率/预算限制）
- 子 agent 的工具调用依然走现有 `SecurityPolicy`（shell、file_write 等都受限），不额外放宽
- 禁止记录/输出密钥：registry 持久化不写入 api_key、token 或工具参数原文
- 深度限制：防止无限递归，每个 agent 配置有 max_depth 限制

## 8. 已实现文件清单

### 8.1 核心模块

- `src/swarm/mod.rs`：
  - SwarmManager：子 agent 生命周期管理
  - SubagentRun：子 agent run 数据模型
  - RunStatus：运行状态枚举
  - SwarmContext：swarm 上下文（深度、是否允许 spawn）

- `src/swarm/queue.rs`：
  - LaneQueue：lane 队列实现
  - LaneState：lane 状态
  - Job：任务封装

- `src/swarm/store.rs`：
  - SwarmSqliteStore：SQLite 持久化存储
  - SwarmEvent：事件数据模型
  - SwarmChatMessage：群聊消息数据模型

- `src/swarm/chat.rs`：
  - SwarmChatManager：群聊通信管理
  - ChatMessage：聊天消息数据模型
  - ChatMessageType：消息类型枚举
  - ConsensusState：共识状态数据模型

- `src/swarm/consensus.rs`：
  - ConsensusManager：共识机制管理
  - TaskConsensus：任务共识数据模型
  - ConsensusProposal：共识提案数据模型
  - DisagreementEntry：分歧条目数据模型
  - ClarificationEntry：澄清条目数据模型

- `src/swarm/dependency.rs`：
  - TaskDependencyManager：任务依赖管理
  - TaskDependency：任务依赖数据模型
  - DependencyGraph：依赖图数据模型
  - TaskNode：任务节点数据模型
  - DependencyEdge：依赖边数据模型
  - DependencyAnalysis：依赖分析结果数据模型
  - TaskCoordinationRequest：协调请求数据模型
  - TaskCoordinationResponse：协调响应数据模型

### 8.2 工具

- `src/tools/sessions_spawn.rs`：
  - SessionsSpawnTool：spawn 子 agent run 的工具

- `src/tools/subagents.rs`：
  - SubagentsTool：管理子 agent run 的工具（list/get/wait/kill/steer）

### 8.3 配置

- `src/config/schema.rs`：
  - SwarmConfig：swarm 配置（subagent_max_concurrent）
  - DelegateAgentConfig：delegate agent 配置（provider、model、system_prompt、temperature、max_depth）

## 9. 测试策略

### 9.1 单元测试

- `src/swarm/queue.rs`：
  - 并发控制
  - FIFO 顺序
  - cancel_pending 行为
  - abort_running 行为

- `src/swarm/mod.rs`：
  - registry 状态机
  - 持久化读写
  - 父子关系管理
  - 级联 kill

- `src/tools/sessions_spawn.rs`：
  - schema 验证
  - 参数校验
  - 安全策略检查

- `src/tools/subagents.rs`：
  - 各 action 的正确性
  - 错误处理
  - 安全策略检查

### 9.2 集成测试

- `tests/agent_swarm_flow.rs`：
  - spawn 两个子任务（subagent lane 并发=2）
  - wait 拉取结果
  - steer 纠偏并产生新 run
  - kill 一个未完成 run
  - 验证 registry 最终状态与输出聚合
  - 群聊通信测试
  - 共识机制测试
  - 任务依赖管理测试

### 9.3 测试原则

- 不依赖外部网络或真实 LLM 服务
- 通过实现一个"完整可运行"的本地 Provider 来驱动 agent/tool 循环（用于测试环境）
- 测试中文和英文语言支持
- 测试并发和竞态条件

## 10. 可观测性

### 10.1 进度跟踪

- ProgressEntry：进度条目数据模型
- ProgressStatus：进度状态枚举
- 支持进度百分比、总数、单位等

### 10.2 跟踪日志

- TraceEntry：跟踪日志条目数据模型
- 支持日志级别（info、warn、error 等）
- 支持中英文语言
- 支持父子关系和元数据

### 10.3 导出功能

- ExportFilter：导出过滤器
- 支持导出为 JSON 和 CSV 格式
- 支持按时间范围、状态、任务等过滤

## 11. 与 Trae Skill 集成

### 11.1 使用场景

ZeroClaw 软件开发团队可以通过 Trae skill：

1. 观察 Trae 的任务执行状态
2. 获取 Trae 的任务结果
3. 向 Trae 发送命令
4. 团队讨论并达成共识
5. 提供反馈给 Trae

### 11.2 集成方式

- 通过 `sessions_spawn` 创建子 agent 来处理 Trae 相关任务
- 通过 `subagents wait` 等待任务完成
- 通过群聊机制进行团队讨论
- 通过共识机制达成团队决策
- 通过任务依赖管理协调多个 Trae 实例

## 12. 总结

ZeroClaw Agent Swarm 实现了完整的智能体群组功能：

1. ✅ spawn：创建并启动子 agent run
2. ✅ registry：记录 run 生命周期与元数据，支持查询/等待
3. ✅ lane 队列：并发控制与 FIFO 排队
4. ✅ kill/steer：终止子 run 或纠偏并重新运行
5. ✅ 群聊通信：智能体之间的消息传递和讨论
6. ✅ 共识机制：团队决策和冲突解决
7. ✅ 任务依赖管理：任务之间的依赖关系和协调
8. ✅ SQLite 持久化：可靠的数据存储
9. ✅ 可观测性：进度跟踪和日志记录
10. ✅ 安全策略：最小权限原则和速率限制

通过这些功能，ZeroClaw 能够支持复杂的智能体群组协作场景，包括软件开发团队协作、多环境部署、任务依赖管理等。

## 13. 智能体使用手册

### 13.1 快速开始

#### 13.1.1 配置智能体群组

首先，在 `config.toml` 中配置智能体群组：

```toml
[agent]
compact_context = false
max_tool_iterations = 20

[agents]
# 技术负责人
[agents.tech_lead]
provider = "glm"
model = "glm-4"
system_prompt = "You are a Technical Lead and Software Architect..."
temperature = 0.3
max_depth = 5

# 后端开发人员
[agents.backend_dev]
provider = "glm"
model = "glm-4"
system_prompt = "You are a Backend Developer..."
temperature = 0.2
max_depth = 4
```

#### 13.1.2 启动 ZeroClaw

```bash
zeroclaw
```

#### 13.1.3 创建子智能体任务

使用 `sessions_spawn` 工具创建子智能体：

```json
{
  "agent": "backend_dev",
  "task": "实现用户认证 API，包括登录、注册和密码重置功能",
  "label": "用户认证模块",
  "orchestrator": false,
  "cleanup": false
}
```

### 13.2 基本操作指南

#### 13.2.1 创建子智能体（sessions_spawn）

**用途**：将任务分配给专门的智能体执行

**参数**：
- `agent`（必填）：智能体名称，对应 config.toml 中的 `[agents.xxx]`
- `task`（必填）：任务描述
- `label`（可选）：任务标签，用于识别和聚合
- `orchestrator`（可选，默认 false）：是否允许该智能体再创建子智能体
- `parent_run_id`（可选）：父任务 ID，用于建立层级关系
- `cleanup`（可选，默认 false）：完成后是否自动清理记录

**示例**：

```json
{
  "agent": "frontend_dev",
  "task": "创建 React 组件用于显示用户列表，支持分页和搜索",
  "label": "用户列表组件",
  "orchestrator": false
}
```

**返回**：
```json
{
  "run_id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "running",
  "hint": "使用 subagents wait 拉取结果"
}
```

#### 13.2.2 管理子智能体（subagents）

**用途**：查询、等待、终止或纠偏子智能体

**参数**：
- `action`：操作类型（list|get|wait|kill|steer）
- `run_id`：除 list 外必填
- `timeout_secs`：仅用于 wait（0 = 无超时）
- `message`：steer 时必填，新的任务描述

**操作类型**：

1. **list**：列出所有子智能体
```json
{
  "action": "list"
}
```

2. **get**：获取特定子智能体详情
```json
{
  "action": "get",
  "run_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

3. **wait**：等待子智能体完成
```json
{
  "action": "wait",
  "run_id": "550e8400-e29b-41d4-a716-446655440000",
  "timeout_secs": 300
}
```

4. **kill**：终止子智能体
```json
{
  "action": "kill",
  "run_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

5. **steer**：纠偏并重新运行
```json
{
  "action": "steer",
  "run_id": "550e8400-e29b-41d4-a716-446655440000",
  "message": "请使用 TypeScript 而不是 JavaScript"
}
```

### 13.3 高级功能使用

#### 13.3.1 群聊通信

智能体之间可以通过群聊进行沟通和协作。

**发送消息**：
```json
{
  "action": "send_message",
  "run_id": "550e8400-e29b-41d4-a716-446655440000",
  "task_id": "task-001",
  "author": "tech_lead",
  "author_type": "agent",
  "message_type": "TaskAssignment",
  "content": "请实现用户认证 API，使用 JWT 进行身份验证",
  "lang": "zh"
}
```

**消息类型**：
- `TaskAssignment`：任务分配
- `TaskStatus`：任务状态更新
- `TaskProgress`：任务进度
- `TaskCompletion`：任务完成
- `TaskFailure`：任务失败
- `ConsensusRequest`：共识请求
- `ConsensusResponse`：共识响应
- `Disagreement`：分歧报告
- `Clarification`：澄清请求/响应
- `Correction`：纠正
- `Info`：信息

#### 13.3.2 共识机制

当团队成员对某个决策有分歧时，可以使用共识机制达成一致。

**发起共识**：
```json
{
  "action": "initiate_consensus",
  "proposal": {
    "task_id": "task-001",
    "topic": "API 认证方案选择",
    "description": "应该使用 JWT 还是 Session 进行 API 身份验证",
    "proposer": "tech_lead",
    "options": ["JWT", "Session", "OAuth2"],
    "deadline_unix": 1234567890
  },
  "lang": "zh"
}
```

**投票**：
```json
{
  "action": "vote",
  "task_id": "task-001",
  "voter": "backend_dev",
  "vote": "JWT",
  "reason": "JWT 更适合 RESTful API，易于扩展"
}
```

**报告分歧**：
```json
{
  "action": "report_disagreement",
  "task_id": "task-001",
  "participant": "frontend_dev",
  "reason": "担心 JWT 的 token 过期处理复杂"
}
```

#### 13.3.3 任务依赖管理

定义和管理任务之间的依赖关系。

**定义依赖**：
```json
{
  "action": "define_dependency",
  "task_id": "task-002",
  "depends_on": ["task-001"],
  "dependency_type": "Sequential",
  "condition": "task-001 完成且状态为 success"
}
```

**依赖类型**：
- `Sequential`：顺序执行
- `Parallel`：并行执行
- `Conditional`：条件执行
- `DataFlow`：数据流依赖
- `ResourceSharing`：资源共享

**获取可执行任务**：
```json
{
  "action": "get_ready_tasks"
}
```

**计算关键路径**：
```json
{
  "action": "calculate_critical_path"
}
```

### 13.4 实际案例

#### 13.4.1 案例 1：开发一个完整的 Web 应用

**场景**：开发一个包含前后端的 Web 应用

**步骤**：

1. **技术负责人设计架构**
```json
{
  "agent": "tech_lead",
  "task": "设计一个电商平台的系统架构，包括前端、后端、数据库和缓存方案",
  "label": "系统架构设计",
  "orchestrator": true
}
```

2. **后端开发人员实现 API**
```json
{
  "agent": "backend_dev",
  "task": "实现商品管理 API，包括 CRUD 操作和搜索功能",
  "label": "商品管理 API",
  "orchestrator": false
}
```

3. **前端开发人员实现界面**
```json
{
  "agent": "frontend_dev",
  "task": "实现商品列表页面，支持分页和筛选",
  "label": "商品列表页面",
  "orchestrator": false
}
```

4. **测试人员编写测试**
```json
{
  "agent": "qa_engineer",
  "task": "为商品管理 API 编写单元测试和集成测试",
  "label": "API 测试",
  "orchestrator": false
}
```

5. **等待所有任务完成**
```json
{
  "action": "list"
}
```

6. **聚合结果并部署**
```json
{
  "agent": "devops_engineer",
  "task": "将前端和后端部署到生产环境",
  "label": "生产部署",
  "orchestrator": false
}
```

#### 13.4.2 案例 2：代码审查和重构

**场景**：对现有代码进行审查和重构

**步骤**：

1. **技术负责人发起代码审查**
```json
{
  "agent": "tech_lead",
  "task": "审查用户认证模块的代码，识别潜在问题和改进点",
  "label": "代码审查",
  "orchestrator": true
}
```

2. **后端开发人员实施重构**
```json
{
  "agent": "backend_dev",
  "task": "根据审查结果重构用户认证模块，提高代码质量和可维护性",
  "label": "代码重构",
  "orchestrator": false
}
```

3. **测试人员验证重构**
```json
{
  "agent": "qa_engineer",
  "task": "验证重构后的代码功能正常，没有引入新的 bug",
  "label": "回归测试",
  "orchestrator": false
}
```

#### 13.4.3 案例 3：多环境部署

**场景**：将应用部署到开发、测试和生产环境

**步骤**：

1. **定义任务依赖**
```json
{
  "action": "define_dependency",
  "task_id": "deploy-test",
  "depends_on": ["deploy-dev"],
  "dependency_type": "Sequential"
}
```

```json
{
  "action": "define_dependency",
  "task_id": "deploy-prod",
  "depends_on": ["deploy-test"],
  "dependency_type": "Sequential"
}
```

2. **部署到开发环境**
```json
{
  "agent": "devops_engineer",
  "task": "部署应用到开发环境",
  "label": "开发环境部署",
  "orchestrator": false
}
```

3. **部署到测试环境**
```json
{
  "agent": "devops_engineer",
  "task": "部署应用到测试环境",
  "label": "测试环境部署",
  "orchestrator": false
}
```

4. **部署到生产环境**
```json
{
  "agent": "devops_engineer",
  "task": "部署应用到生产环境",
  "label": "生产环境部署",
  "orchestrator": false
}
```

### 13.5 最佳实践

#### 13.5.1 任务分配

1. **明确任务目标**：任务描述应该清晰、具体、可衡量
2. **选择合适的智能体**：根据任务类型选择最专业的智能体
3. **设置合理的深度**：`orchestrator=true` 只在需要时使用，避免无限递归
4. **使用标签**：为任务设置有意义的标签，便于后续管理和聚合

#### 13.5.2 协作沟通

1. **及时更新状态**：使用群聊及时通知其他智能体任务进展
2. **主动寻求共识**：在重要决策前发起共识，确保团队一致
3. **清晰表达分歧**：使用 `Disagreement` 消息类型明确表达不同意见
4. **请求澄清**：遇到不确定的地方使用 `Clarification` 消息类型

#### 13.5.3 依赖管理

1. **合理设计依赖**：避免循环依赖，保持依赖图为 DAG
2. **并行化独立任务**：使用 `Parallel` 依赖类型提高效率
3. **监控关键路径**：定期计算关键路径，识别瓶颈
4. **处理阻塞任务**：及时处理阻塞的任务，避免影响整体进度

#### 13.5.4 错误处理

1. **记录错误信息**：使用 `TaskFailure` 消息类型记录失败原因
2. **尝试恢复**：对于可恢复的错误，使用 `steer` 进行纠偏
3. **及时终止**：对于无法恢复的任务，使用 `kill` 终止
4. **分析根因**：分析失败原因，改进流程

### 13.6 故障排查

#### 13.6.1 子智能体无响应

**症状**：子智能体状态一直为 `Running`，但没有输出

**排查步骤**：

1. 检查子智能体状态：
```json
{
  "action": "get",
  "run_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

2. 检查事件日志：
```bash
sqlite3 ~/.zeroclaw/swarm.db "SELECT * FROM swarm_events WHERE run_id = '550e8400-e29b-41d4-a716-446655440000' ORDER BY ts_unix DESC LIMIT 10"
```

3. 检查心跳时间：
```bash
sqlite3 ~/.zeroclaw/swarm.db "SELECT run_id, last_heartbeat_unix FROM subagent_runs WHERE run_id = '550e8400-e29b-41d4-a716-446655440000'"
```

**解决方案**：

- 如果心跳超时，使用 `kill` 终止并重新创建
- 检查任务描述是否过于复杂，尝试拆分任务
- 增加 `max_tool_iterations` 配置值

#### 13.6.2 任务执行失败

**症状**：子智能体状态为 `Failed`

**排查步骤**：

1. 获取失败详情：
```json
{
  "action": "get",
  "run_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

2. 查看错误信息：
```bash
sqlite3 ~/.zeroclaw/swarm.db "SELECT error FROM subagent_runs WHERE run_id = '550e8400-e29b-41d4-a716-446655440000'"
```

**解决方案**：

- 分析错误原因，使用 `steer` 进行纠偏
- 检查任务描述是否清晰
- 确认智能体配置是否正确（provider、model、system_prompt）

#### 13.6.3 共识无法达成

**症状**：共识状态一直为 `Pending` 或 `Disagreement`

**排查步骤**：

1. 查看共识详情：
```bash
sqlite3 ~/.zeroclaw/swarm.db "SELECT * FROM task_consensus WHERE task_id = 'task-001'"
```

2. 查看投票情况：
```bash
sqlite3 ~/.zeroclaw/swarm.db "SELECT votes_json FROM task_consensus WHERE task_id = 'task-001'"
```

**解决方案**：

- 检查所有参与者是否都已投票
- 使用 `request_clarification` 澄清分歧点
- 调整提案选项，使其更明确
- 设置合理的截止时间

#### 13.6.4 依赖任务阻塞

**症状**：任务状态一直为 `Blocked`

**排查步骤**：

1. 查看依赖关系：
```bash
sqlite3 ~/.zeroclaw/swarm.db "SELECT * FROM task_dependencies WHERE task_id = 'task-002'"
```

2. 查看阻塞原因：
```bash
sqlite3 ~/.zeroclaw/swarm.db "SELECT blocking_reason FROM task_dependencies WHERE task_id = 'task-002'"
```

**解决方案**：

- 检查依赖任务是否已完成
- 验证依赖条件是否满足
- 考虑调整依赖关系
- 使用 `calculate_critical_path` 识别关键路径

### 13.7 性能优化

#### 13.7.1 并发控制

调整并发控制参数：

```toml
[swarm]
subagent_max_concurrent = 5  # 增加并发数
```

#### 13.7.2 工具调用限制

调整工具调用最大迭代次数：

```toml
[agent]
max_tool_iterations = 30  # 增加工具调用次数
```

#### 13.7.3 数据库优化

启用 WAL 模式以提高并发性能：

```bash
sqlite3 ~/.zeroclaw/swarm.db "PRAGMA journal_mode=WAL"
```

定期清理旧数据：

```bash
sqlite3 ~/.zeroclaw/swarm.db "DELETE FROM swarm_events WHERE ts_unix < strftime('%s', 'now', '-7 days') * 1000"
```

### 13.8 监控和日志

#### 13.8.1 查看运行状态

```bash
# 列出所有子智能体
zeroclaw --action list

# 查看特定子智能体
zeroclaw --action get --run-id <run_id>
```

#### 13.8.2 查看事件日志

```bash
# 查看最近的事件
sqlite3 ~/.zeroclaw/swarm.db "SELECT * FROM swarm_events ORDER BY ts_unix DESC LIMIT 20"

# 查看特定运行的事件
sqlite3 ~/.zeroclaw/swarm.db "SELECT * FROM swarm_events WHERE run_id = '<run_id>' ORDER BY ts_unix"
```

#### 13.8.3 查看群聊消息

```bash
# 查看最近的群聊消息
sqlite3 ~/.zeroclaw/swarm.db "SELECT * FROM swarm_chat_extended ORDER BY ts_unix DESC LIMIT 20"

# 查看特定任务的群聊消息
sqlite3 ~/.zeroclaw/swarm.db "SELECT * FROM swarm_chat_extended WHERE task_id = '<task_id>' ORDER BY ts_unix"
```

#### 13.8.4 查看进度和跟踪

```bash
# 查看进度条目
sqlite3 ~/.zeroclaw/swarm.db "SELECT * FROM progress_entries ORDER BY updated_at DESC"

# 查看跟踪日志
sqlite3 ~/.zeroclaw/swarm.db "SELECT * FROM trace_entries ORDER BY timestamp DESC LIMIT 50"
```

### 13.9 安全建议

1. **限制权限**：确保每个智能体只拥有完成任务所需的最小权限
2. **审查任务**：在执行任务前审查任务描述，避免恶意操作
3. **监控活动**：定期检查事件日志，识别异常行为
4. **清理记录**：定期清理旧的敏感数据
5. **使用 HTTPS**：确保所有网络通信都使用 HTTPS

### 13.10 常见问题

**Q1：如何限制子智能体的资源使用？**

A：可以通过配置 `max_depth` 和 `max_tool_iterations` 来限制子智能体的资源使用。

**Q2：如何处理子智能体之间的冲突？**

A：使用共识机制和群聊通信来协调和解决冲突。

**Q3：如何确保子智能体的输出质量？**

A：设置合适的 `temperature` 参数，使用明确的任务描述，并通过代码审查来验证输出。

**Q4：如何调试子智能体的执行过程？**

A：查看事件日志、跟踪日志和群聊消息，了解子智能体的执行过程。

**Q5：如何优化子智能体的执行速度？**

A：增加并发数、优化任务描述、使用更快的模型、减少不必要的工具调用。
