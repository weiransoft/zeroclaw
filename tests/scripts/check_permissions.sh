#!/bin/bash

# macOS 辅助功能权限检查脚本

echo "======================================"
echo "macOS 辅助功能权限检查"
echo "======================================"
echo ""

# 检查 1：查看当前 macOS 版本
echo "检查 1：macOS 版本"
echo "--------------------------------------"
sw_vers
echo ""

# 检查 2：查看辅助功能权限
echo "检查 2：辅助功能权限"
echo "--------------------------------------"
sudo tccutil list Accessibility
echo ""

# 检查 3：测试 osascript 命令
echo "检查 3：测试 osascript 命令"
echo "--------------------------------------"
echo "测试 1：切换应用"
osascript -e 'tell application "System Events" to keystroke "Terminal" using command down' 2>&1
echo ""

echo "测试 2：按下 Enter 键"
osascript -e 'tell application "System Events" to key code 36' 2>&1
echo ""

echo "测试 3：输入文本"
osascript -e 'tell application "System Events" to keystroke "测试文本"' 2>&1
echo ""

# 检查 4：查看系统日志
echo "检查 4：查看系统日志（最近 1 小时）"
echo "--------------------------------------"
log show --predicate 'eventMessage contains "accessibility"' --info --last 1h | tail -20
echo ""

echo "======================================"
echo "检查完成"
echo "======================================"