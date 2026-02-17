# Agent Swarm — Design and Implementation Blueprint (ZeroClaw)

This document describes the complete implementation of "agent swarm" in ZeroClaw, including spawn, run registry, lane concurrent queue, wait/kill/steer, result return constraints, as well as group chat communication, consensus mechanism, and task dependency management.

## 1. Goals and Non-Goals

### 1.1 Goals

- Enable the main agent to split a task into multiple sub-tasks and execute them concurrently in a "group/orchestration" manner, then aggregate and output results
- Support key loops:
  - Spawn sub-agent runs (with label, depth, optional orchestrator/leaf constraints)
  - Registry records run lifecycle and metadata, and supports query/wait
  - Lane queue and concurrency control (concepts of main/subagent/nested)
  - Kill/steer: terminate sub runs or correct their input and re-run
  - Group chat communication: message passing and discussion between agents
  - Consensus mechanism: team decision-making and conflict resolution
  - Task dependency management: dependencies and coordination between tasks
- Strictly follow ZeroClaw architectural principles: trait + factory as extension points, avoid cross-subsystem coupling; secure by default minimum privilege; explicit failure

### 1.2 Non-Goals

- Do not implement cross-process/cross-machine swarm distributed scheduling (this design focuses on in-process lane queues and task orchestration)
- Do not implement "real-time announce queue for async push to LLM history" (use explicit wait/list pull and optional memory recording as the loop)

## 2. Overall Architecture

### 2.1 Core Components

#### SwarmManager
- Responsibility: Manage sub-agent lifecycle, registry, and queue scheduling
- Functions:
  - spawn: Create and start sub-agent runs
  - list: List all sub-agent runs
  - get: Get detailed information of a specific run
  - wait: Wait for run completion (with timeout support)
  - kill: Terminate run (cascade terminate descendants)
  - steer: Correct and re-run
- Persistence: Store to SQLite database via SwarmSqliteStore

#### LaneQueue
- Responsibility: Control concurrency and FIFO queuing by lane
- Functions:
  - enqueue: Submit task to specified lane
  - cancel_pending: Cancel pending tasks in the queue
  - abort_running: Abort running tasks
- Lane types:
  - `subagent`: Sub-agent concurrent pool
  - Extensible: `main`, `nested`, etc.

#### SwarmSqliteStore
- Responsibility: Provide SQLite persistent storage
- Functions:
  - CRUD operations for sub-agent runs
  - Event log recording
  - Group chat message storage
  - Consensus data storage
  - Task dependency storage
  - Progress and trace data storage
- Database path: `<workspace_dir>/.zeroclaw/swarm.db`

#### SwarmChatManager
- Responsibility: Manage group chat communication between agents
- Functions:
  - send_message: Send message
  - send_task_assignment: Send task assignment
  - send_task_status: Send task status update
  - request_consensus: Initiate consensus request
  - respond_consensus: Respond to consensus request
  - report_disagreement: Report disagreement
  - request_clarification: Request clarification
  - respond_clarification: Respond to clarification

#### ConsensusManager
- Responsibility: Manage team consensus mechanism
- Functions:
  - initiate_consensus: Initiate consensus
  - vote: Vote
  - report_disagreement: Report disagreement
  - request_clarification: Request clarification
  - respond_clarification: Respond to clarification
  - check_consensus: Check if consensus is reached

#### TaskDependencyManager
- Responsibility: Manage dependencies between tasks
- Functions:
  - define_dependency: Define task dependency
  - analyze_dependencies: Analyze dependencies
  - get_ready_tasks: Get executable tasks
  - update_task_status: Update task status
  - calculate_critical_path: Calculate critical path
  - get_coordination_requests: Get coordination requests

### 2.2 Data Flow

1) Main agent (or user) triggers `sessions_spawn` (JSON parameters include: agent, task, label, requester, cleanup, orchestrator, max_depth, etc.)

2) After `sessions_spawn` passes `SecurityPolicy` validation:
   - Create run record in registry (status=Pending)
   - Enqueue execution closure to lane=`subagent`

3) Lane queue starts execution:
   - Based on sub-agent configuration in Config.agents (provider/model/temp/system_prompt)
   - Construct sub-agent system prompt
   - Reuse ZeroClaw's existing `agent::process_message` logic
   - Ensure tool calls are consistent with security policy

4) After sub-run completion:
   - Registry updates status=Completed/Failed
   - Persist results to SQLite database
   - Send group chat message to notify related agents

5) Main agent pulls results via `subagents wait` or `subagents list` and aggregates output

### 2.3 Group Chat Communication Flow

1) Agents send messages through SwarmChatManager
2) Messages are stored in the swarm_chat_extended table of SQLite database
3) Other agents can query and respond to messages
4) Supported message types:
   - TaskAssignment: Task assignment
   - TaskStatus: Task status
   - TaskProgress: Task progress
   - TaskCompletion: Task completion
   - TaskFailure: Task failure
   - ConsensusRequest: Consensus request
   - ConsensusResponse: Consensus response
   - Disagreement: Disagreement report
   - Clarification: Clarification request/response
   - Correction: Correction
   - Info: Information

### 2.4 Consensus Mechanism Flow

1) Agent initiates consensus request (ConsensusManager.initiate_consensus)
2) Participating agents receive request and vote (ConsensusManager.vote)
3) If there is disagreement, agents can report disagreement (ConsensusManager.report_disagreement)
4) Can request clarification (ConsensusManager.request_clarification)
5) Finally reach consensus or fail

### 2.5 Task Dependency Coordination Flow

1) Define task dependencies (TaskDependencyManager.define_dependency)
2) Analyze dependencies (TaskDependencyManager.analyze_dependencies)
3) Get executable tasks (TaskDependencyManager.get_ready_tasks)
4) Update task status after completion (TaskDependencyManager.update_task_status)
5) Calculate critical path (TaskDependencyManager.calculate_critical_path)

## 3. Sub-Agent System Prompt (Constraints)

Goal: Solidify rules of "only do tasks, avoid deviation, avoid active external connection" as system-level constraints

Prompt structure:

- Identity: You are a sub-agent of ZeroClaw, task is `<label>`
- Input source: requester (optional, string, do not write privacy/keys)
- Depth and orchestration: If `orchestrator=true`, allow spawning sub-agents; otherwise prohibit spawn (prevent infinite recursion)
- Output requirements: Must produce structured results that can be aggregated by upper layer (suggestion: title + bullet points + risks/dependencies)
- Language support: Support Chinese and English communication

## 4. Concurrency and Lane Design

### 4.1 Lane

- `subagent`: Sub-agent concurrent pool
- Extensible: `main`, `nested`, etc.

### 4.2 LaneQueue Behavior

- One `Semaphore` per lane to control maximum concurrency
- One FIFO queue (`VecDeque`) per lane, managed by a separate pump task that takes tasks and executes under `Semaphore` permit
- Task execution returns `oneshot` result
- Kill/abort:
  - For "not started tasks": remove from queue and mark as Cancelled
  - For "running tasks": terminate tokio task via `AbortHandle` and mark as Terminated

## 5. Registry and Persistence

### 5.1 Run Data Model (Core Fields)

- `run_id`: UUID
- `parent_run_id`: Optional
- `agent_name`: From key in `Config.agents`
- `label`: Optional
- `task`: Sub-task text
- `status`: Pending/Running/Completed/Failed/Cancelled/Terminated
- `started_at_unix`/`ended_at_unix`
- `output`: Completion output (optional)
- `error`: Failure reason (optional)
- `depth`: Spawn depth
- `children`: Sub-run list (for cascade kill)
- `cleanup`: Whether to automatically clean up after completion
- `orchestrator`: Whether to allow spawning again
- `owner_instance`: Instance ID it belongs to
- `last_heartbeat_unix`: Last heartbeat time

### 5.2 Persistence Location

- Database: `<workspace_dir>/.zeroclaw/swarm.db`
- Database type: SQLite (supports WAL mode, concurrent access)
- Write strategy: Write immediately after each status change (guarantee atomicity via SQLite transactions)
- Read strategy: Lazy load on startup (load when swarm is first used)
- Migration support: Automatically migrate from old JSON file (subagents.json) to SQLite

### 5.3 Database Schema

#### subagent_runs table
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

#### swarm_events table
```sql
CREATE TABLE swarm_events (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    ts_unix INTEGER NOT NULL,
    run_id  TEXT,
    kind    TEXT NOT NULL,
    payload TEXT NOT NULL
);
```

#### swarm_chat_extended table
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

#### task_consensus table
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

#### task_dependencies table
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

#### progress_entries table
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

#### trace_entries table
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

## 6. Tool Interface Design

### 6.1 sessions_spawn

Parameters (JSON):

- `agent` (required): Delegate agent name (corresponds to `config.agents` key)
- `task` (required): Sub-task text
- `label` (optional): For upper-layer aggregation and display
- `orchestrator` (optional, default false): Whether to allow this sub-agent to spawn again
- `parent_run_id` (optional): For establishing parent-child relationship (upper layer can pass in itself)
- `cleanup` (optional, default false): Whether to automatically clean up registry record after completion

Returns:

- `run_id`
- `status` (running)
- `hint`: Hint for upper layer to use `subagents wait` to pull results

### 6.2 subagents

Parameters (JSON):

- `action`: `list|get|wait|kill|steer`
- `run_id`: Required except for list
- `timeout_secs`: Only for wait (0 = no timeout)
- `message`: Required for steer (as new task/correction input)

Returns:

- list: List of run summaries (id/agent/status/label/age)
- get/wait: Detailed information including output/error
- kill: Confirmation result (whether terminate/cancel succeeded)
- steer: New run_id (replacement strategy for old run: mark old run as Terminated, new run inherits parent/label)

## 7. Security Policy and Limitations

- `sessions_spawn`, `kill`, `steer` all belong to "actions that trigger side effects", must:
  - `security.can_act()` is true
  - `security.record_action()` succeeds (rate/budget limits)
- Tool calls of sub-agents still follow existing `SecurityPolicy` (shell, file_write, etc. are all restricted), no additional relaxation
- Prohibit recording/outputting keys: Registry persistence does not write api_key, token or tool parameter original text
- Depth limit: Prevent infinite recursion, each agent configuration has max_depth limit

## 8. Implemented File List

### 8.1 Core Modules

- `src/swarm/mod.rs`:
  - SwarmManager: Sub-agent lifecycle management
  - SubagentRun: Sub-agent run data model
  - RunStatus: Run status enum
  - SwarmContext: Swarm context (depth, whether to allow spawn)

- `src/swarm/queue.rs`:
  - LaneQueue: Lane queue implementation
  - LaneState: Lane state
  - Job: Task wrapper

- `src/swarm/store.rs`:
  - SwarmSqliteStore: SQLite persistent storage
  - SwarmEvent: Event data model
  - SwarmChatMessage: Group chat message data model

- `src/swarm/chat.rs`:
  - SwarmChatManager: Group chat communication management
  - ChatMessage: Chat message data model
  - ChatMessageType: Message type enum
  - ConsensusState: Consensus state data model

- `src/swarm/consensus.rs`:
  - ConsensusManager: Consensus mechanism management
  - TaskConsensus: Task consensus data model
  - ConsensusProposal: Consensus proposal data model
  - DisagreementEntry: Disagreement entry data model
  - ClarificationEntry: Clarification entry data model

- `src/swarm/dependency.rs`:
  - TaskDependencyManager: Task dependency management
  - TaskDependency: Task dependency data model
  - DependencyGraph: Dependency graph data model
  - TaskNode: Task node data model
  - DependencyEdge: Dependency edge data model
  - DependencyAnalysis: Dependency analysis result data model
  - TaskCoordinationRequest: Coordination request data model
  - TaskCoordinationResponse: Coordination response data model

### 8.2 Tools

- `src/tools/sessions_spawn.rs`:
  - SessionsSpawnTool: Tool for spawning sub-agent runs

- `src/tools/subagents.rs`:
  - SubagentsTool: Tool for managing sub-agent runs (list/get/wait/kill/steer)

### 8.3 Configuration

- `src/config/schema.rs`:
  - SwarmConfig: Swarm configuration (subagent_max_concurrent)
  - DelegateAgentConfig: Delegate agent configuration (provider, model, system_prompt, temperature, max_depth)

## 9. Testing Strategy

### 9.1 Unit Tests

- `src/swarm/queue.rs`:
  - Concurrency control
  - FIFO order
  - cancel_pending behavior
  - abort_running behavior

- `src/swarm/mod.rs`:
  - Registry state machine
  - Persistence read/write
  - Parent-child relationship management
  - Cascade kill

- `src/tools/sessions_spawn.rs`:
  - Schema validation
  - Parameter validation
  - Security policy check

- `src/tools/subagents.rs`:
  - Correctness of each action
  - Error handling
  - Security policy check

### 9.2 Integration Tests

- `tests/agent_swarm_flow.rs`:
  - Spawn two sub-tasks (subagent lane concurrent=2)
  - Wait to pull results
  - Steer to correct and produce new run
  - Kill one unfinished run
  - Verify registry final state and output aggregation
  - Group chat communication test
  - Consensus mechanism test
  - Task dependency management test

### 9.3 Testing Principles

- Do not depend on external network or real LLM services
- Drive agent/tool loops by implementing a "fully runnable" local Provider (for testing environment)
- Test Chinese and English language support
- Test concurrency and race conditions

## 10. Observability

### 10.1 Progress Tracking

- ProgressEntry: Progress entry data model
- ProgressStatus: Progress status enum
- Supports progress percentage, total, unit, etc.

### 10.2 Trace Logging

- TraceEntry: Trace log entry data model
- Supports log levels (info, warn, error, etc.)
- Supports Chinese and English languages
- Supports parent-child relationships and metadata

### 10.3 Export Functionality

- ExportFilter: Export filter
- Supports export to JSON and CSV formats
- Supports filtering by time range, status, task, etc.

## 11. Integration with Trae Skill

### 11.1 Use Cases

ZeroClaw software development team can use Trae skill to:

1. Observe Trae's task execution status
2. Get Trae's task results
3. Send commands to Trae
4. Team discussion and reach consensus
5. Provide feedback to Trae

### 11.2 Integration Method

- Create sub-agents via `sessions_spawn` to handle Trae-related tasks
- Wait for task completion via `subagents wait`
- Conduct team discussion via group chat mechanism
- Reach team decisions via consensus mechanism
- Coordinate multiple Trae instances via task dependency management

## 12. Summary

ZeroClaw Agent Swarm implements complete agent swarm functionality:

1. ✅ spawn: Create and start sub-agent runs
2. ✅ registry: Record run lifecycle and metadata, support query/wait
3. ✅ lane queue: Concurrency control and FIFO queuing
4. ✅ kill/steer: Terminate sub runs or correct and re-run
5. ✅ group chat communication: Message passing and discussion between agents
6. ✅ consensus mechanism: Team decision-making and conflict resolution
7. ✅ task dependency management: Dependencies and coordination between tasks
8. ✅ SQLite persistence: Reliable data storage
9. ✅ observability: Progress tracking and logging
10. ✅ security policy: Minimum privilege principle and rate limiting

Through these features, ZeroClaw can support complex agent swarm collaboration scenarios, including software development team collaboration, multi-environment deployment, task dependency management, etc.

## 13. Agent Usage Manual

### 13.1 Quick Start

#### 13.1.1 Configure Agent Swarm

First, configure agent swarm in `config.toml`:

```toml
[agent]
compact_context = false
max_tool_iterations = 20

[agents]
# Technical Lead
[agents.tech_lead]
provider = "glm"
model = "glm-4"
system_prompt = "You are a Technical Lead and Software Architect..."
temperature = 0.3
max_depth = 5

# Backend Developer
[agents.backend_dev]
provider = "glm"
model = "glm-4"
system_prompt = "You are a Backend Developer..."
temperature = 0.2
max_depth = 4
```

#### 13.1.2 Start ZeroClaw

```bash
zeroclaw
```

#### 13.1.3 Create Sub-Agent Tasks

Use `sessions_spawn` tool to create sub-agents:

```json
{
  "agent": "backend_dev",
  "task": "Implement user authentication API, including login, registration and password reset",
  "label": "User Authentication Module",
  "orchestrator": false,
  "cleanup": false
}
```

### 13.2 Basic Operation Guide

#### 13.2.1 Create Sub-Agent (sessions_spawn)

**Purpose**: Assign tasks to specialized agents for execution

**Parameters**:
- `agent` (required): Agent name, corresponding to `[agents.xxx]` in config.toml
- `task` (required): Task description
- `label` (optional): Task label, for identification and aggregation
- `orchestrator` (optional, default false): Whether to allow this agent to create sub-agents again
- `parent_run_id` (optional): Parent task ID, for establishing hierarchical relationship
- `cleanup` (optional, default false): Whether to automatically clean up records after completion

**Example**:

```json
{
  "agent": "frontend_dev",
  "task": "Create React component to display user list, with pagination and search support",
  "label": "User List Component",
  "orchestrator": false
}
```

**Returns**:
```json
{
  "run_id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "running",
  "hint": "Use subagents wait to pull results"
}
```

#### 13.2.2 Manage Sub-Agents (subagents)

**Purpose**: Query, wait, terminate, or correct sub-agents

**Parameters**:
- `action`: Operation type (list|get|wait|kill|steer)
- `run_id`: Required except for list
- `timeout_secs`: Only for wait (0 = no timeout)
- `message`: Required for steer, new task description

**Operation Types**:

1. **list**: List all sub-agents
```json
{
  "action": "list"
}
```

2. **get**: Get details of specific sub-agent
```json
{
  "action": "get",
  "run_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

3. **wait**: Wait for sub-agent completion
```json
{
  "action": "wait",
  "run_id": "550e8400-e29b-41d4-a716-446655440000",
  "timeout_secs": 300
}
```

4. **kill**: Terminate sub-agent
```json
{
  "action": "kill",
  "run_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

5. **steer**: Correct and re-run
```json
{
  "action": "steer",
  "run_id": "550e8400-e29b-41d4-a716-446655440000",
  "message": "Please use TypeScript instead of JavaScript"
}
```

### 13.3 Advanced Features Usage

#### 13.3.1 Group Chat Communication

Agents can communicate and collaborate through group chat.

**Send Message**:
```json
{
  "action": "send_message",
  "run_id": "550e8400-e29b-41d4-a716-446655440000",
  "task_id": "task-001",
  "author": "tech_lead",
  "author_type": "agent",
  "message_type": "TaskAssignment",
  "lang": "en",
  "content": "Please implement the user authentication API"
}
```

**Supported Message Types**:
- `TaskAssignment`: Task assignment
- `TaskStatus`: Task status
- `TaskProgress`: Task progress
- `TaskCompletion`: Task completion
- `TaskFailure`: Task failure
- `ConsensusRequest`: Consensus request
- `ConsensusResponse`: Consensus response
- `Disagreement`: Disagreement report
- `Clarification`: Clarification request/response
- `Correction`: Correction
- `Info`: Information

#### 13.3.2 Consensus Mechanism

Agents can reach consensus through voting and discussion.

**Initiate Consensus**:
```json
{
  "action": "initiate_consensus",
  "task_id": "task-001",
  "topic": "API Design",
  "participants": ["backend_dev", "frontend_dev", "tech_lead"]
}
```

**Vote**:
```json
{
  "action": "vote",
  "consensus_id": "consensus-001",
  "vote": "approve",
  "reason": "This design is reasonable"
}
```

**Report Disagreement**:
```json
{
  "action": "report_disagreement",
  "consensus_id": "consensus-001",
  "disagreement": "I think we should use REST instead of GraphQL"
}
```

#### 13.3.3 Task Dependency Management

Agents can manage dependencies between tasks.

**Define Dependency**:
```json
{
  "action": "define_dependency",
  "task_id": "task-002",
  "depends_on": "task-001",
  "dependency_type": "sequential"
}
```

**Analyze Dependencies**:
```json
{
  "action": "analyze_dependencies",
  "task_ids": ["task-001", "task-002", "task-003"]
}
```

**Get Ready Tasks**:
```json
{
  "action": "get_ready_tasks"
}
```

**Calculate Critical Path**:
```json
{
  "action": "calculate_critical_path"
}
```

## 14. Example Scenarios

### 14.1 Software Development Team

**Scenario**: Develop a To-Do List application

**Steps**:

1. Product Manager defines requirements
2. Tech Lead designs architecture
3. Backend Developer implements API
4. Frontend Developer implements UI
5. QA Engineer tests
6. DevOps Engineer deploys

**Example Command**:
```bash
zeroclaw agent --message "Please coordinate the team to develop a To-Do List application, including backend API and frontend UI"
```

### 14.2 Multi-Agent Collaboration

**Scenario**: Multiple agents discuss and reach consensus on a technical decision

**Steps**:

1. Tech Lead initiates discussion
2. Backend Developer and Frontend Developer provide opinions
3. Team reaches consensus through voting
4. Implement the agreed solution

**Example Command**:
```bash
zeroclaw agent --message "Please coordinate backend_dev and frontend_dev to discuss and reach consensus on the API design"
```

### 14.3 Task Dependency Management

**Scenario**: Execute tasks with dependencies

**Steps**:

1. Define task dependencies
2. Execute tasks in order
3. Monitor progress
4. Handle failures

**Example Command**:
```bash
zeroclaw agent --message "Please manage the development tasks with dependencies: database design -> API implementation -> UI development -> testing"
```

## 15. Best Practices

### 15.1 Agent Configuration

- Set appropriate `temperature` for each agent based on its role
- Set appropriate `max_depth` to prevent infinite recursion
- Use clear and specific `system_prompt` for each agent
- Configure `orchestrator` carefully to control spawning behavior

### 15.2 Task Assignment

- Break down complex tasks into smaller, manageable sub-tasks
- Use descriptive `label` for easy identification and aggregation
- Set appropriate `orchestrator` and `cleanup` flags
- Use `parent_run_id` to establish clear hierarchical relationships

### 15.3 Communication and Collaboration

- Use group chat for team communication
- Use consensus mechanism for important decisions
- Use task dependency management for complex workflows
- Monitor progress and handle failures promptly

### 15.4 Security and Performance

- Follow security policies and rate limits
- Monitor resource usage and optimize performance
- Clean up completed tasks to save resources
- Use appropriate concurrency limits

## 16. Troubleshooting

### 16.1 Common Issues

**Issue**: Sub-agent not starting
- Check agent configuration in config.toml
- Verify provider and model are correct
- Check security policy settings

**Issue**: Sub-agent stuck in running state
- Check if task is actually executing
- Verify timeout settings
- Use `kill` action to terminate if needed

**Issue**: Consensus not reached
- Check if all participants have voted
- Review disagreements and clarifications
- Consider adjusting consensus threshold

**Issue**: Task dependencies not resolving
- Verify dependency definitions are correct
- Check task status updates
- Review dependency analysis results

### 16.2 Debugging

- Use `subagents list` to view all sub-agents
- Use `subagents get` to view detailed information
- Check database logs for errors
- Enable trace logging for detailed information

## 17. API Reference

### 17.1 Sessions Spawn Tool

**Endpoint**: `sessions_spawn`

**Method**: POST

**Parameters**:
```json
{
  "agent": "string (required)",
  "task": "string (required)",
  "label": "string (optional)",
  "orchestrator": "boolean (optional, default false)",
  "parent_run_id": "string (optional)",
  "cleanup": "boolean (optional, default false)"
}
```

**Response**:
```json
{
  "run_id": "string",
  "status": "string",
  "hint": "string"
}
```

### 17.2 Subagents Tool

**Endpoint**: `subagents`

**Method**: POST

**Parameters**:
```json
{
  "action": "string (required)",
  "run_id": "string (optional)",
  "timeout_secs": "number (optional)",
  "message": "string (optional)"
}
```

**Response** (varies by action):
```json
{
  "result": "object"
}
```

## 18. Configuration Reference

### 18.1 Agent Configuration

```toml
[agents.<agent_name>]
provider = "string (required)"
model = "string (required)"
system_prompt = "string (required)"
temperature = "number (optional, default 0.7)"
max_depth = "number (optional, default 3)"
```

### 18.2 Swarm Configuration

```toml
[swarm]
subagent_max_concurrent = "number (optional, default 3)"
```

### 18.3 Lane Configuration

```toml
[lanes.<lane_name>]
max_concurrent = "number (optional, default 3)"
timeout = "number (optional, default 3600)"
```

## 19. Performance Considerations

### 19.1 Concurrency

- Adjust `subagent_max_concurrent` based on system resources
- Use appropriate lane configuration for different task types
- Monitor queue length and adjust as needed

### 19.2 Persistence

- SQLite WAL mode provides good concurrency performance
- Consider batch writes for high-volume scenarios
- Use indexes for frequently queried fields

### 19.3 Memory Usage

- Clean up completed tasks to save memory
- Use appropriate `cleanup` settings
- Monitor memory usage and adjust as needed

## 20. Future Enhancements

### 20.1 Planned Features

- Cross-process swarm scheduling
- Real-time announce queue
- Advanced consensus algorithms
- Machine learning-based task scheduling
- Distributed task execution

### 20.2 Community Contributions

We welcome contributions to improve the agent swarm functionality. Please follow the contribution guidelines in CONTRIBUTING.md.

---

**Document Version**: 1.0
**Created**: 2026-02-17
**Last Updated**: 2026-02-17
**Maintainer**: ZeroClaw Team