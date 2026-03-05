#!/bin/bash

# ZeroClaw 智能体群组测试脚本

echo "======================================"
echo "ZeroClaw 智能体群组测试"
echo "======================================"
echo ""

# 测试 1：列出可用的智能体
echo "测试 1：列出可用的智能体"
echo "--------------------------------------"
zeroclaw agent --message "请使用 delegate 工具列出所有可用的智能体"
echo ""

# 测试 2：测试 Tech Lead 智能体
echo "测试 2：测试 Tech Lead 智能体"
echo "--------------------------------------"
zeroclaw agent --message "请使用 delegate 工具将任务委托给 tech_lead 智能体，任务内容：'你好，请介绍一下你的角色和职责'"
echo ""

# 测试 3：测试 Backend Developer 智能体
echo "测试 3：测试 Backend Developer 智能体"
echo "--------------------------------------"
zeroclaw agent --message "请使用 delegate 工具将任务委托给 backend_dev 智能体，任务内容：'你好，请介绍一下你的角色和职责'"
echo ""

# 测试 4：测试 Frontend Developer 智能体
echo "测试 4：测试 Frontend Developer 智能体"
echo "--------------------------------------"
zeroclaw agent --message "请使用 delegate 工具将任务委托给 frontend_dev 智能体，任务内容：'你好，请介绍一下你的角色和职责'"
echo ""

# 测试 5：测试 DevOps Engineer 智能体
echo "测试 5：测试 DevOps Engineer 智能体"
echo "--------------------------------------"
zeroclaw agent --message "请使用 delegate 工具将任务委托给 devops 智能体，任务内容：'你好，请介绍一下你的角色和职责'"
echo ""

# 测试 6：测试 QA Engineer 智能体
echo "测试 6：测试 QA Engineer 智能体"
echo "--------------------------------------"
zeroclaw agent --message "请使用 delegate 工具将任务委托给 qa_engineer 智能体，任务内容：'你好，请介绍一下你的角色和职责'"
echo ""

# 测试 7：测试 Security Engineer 智能体
echo "测试 7：测试 Security Engineer 智能体"
echo "--------------------------------------"
zeroclaw agent --message "请使用 delegate 工具将任务委托给 security_engineer 智能体，任务内容：'你好，请介绍一下你的角色和职责'"
echo ""

# 测试 8：测试 Product Manager 智能体
echo "测试 8：测试 Product Manager 智能体"
echo "--------------------------------------"
zeroclaw agent --message "请使用 delegate 工具将任务委托给 product_manager 智能体，任务内容：'你好，请介绍一下你的角色和职责'"
echo ""

# 测试 9：测试 Technical Writer 智能体
echo "测试 9：测试 Technical Writer 智能体"
echo "--------------------------------------"
zeroclaw agent --message "请使用 delegate 工具将任务委托给 tech_writer 智能体，任务内容：'你好，请介绍一下你的角色和职责'"
echo ""

# 测试 10：测试智能体协作
echo "测试 10：测试智能体协作"
echo "--------------------------------------"
zeroclaw agent --message "请使用 delegate 工具协调团队开发一个简单的 To-Do List 应用，包括后端 API 和前端界面。首先请 tech_lead 设计架构，然后请 backend_dev 实现 API，最后请 frontend_dev 创建界面。"
echo ""

echo "======================================"
echo "测试完成"
echo "======================================"