#!/bin/bash

# macOS 辅助功能权限检查脚本（无 sudo）

echo "======================================"
echo "macOS 辅助功能权限检查（无 sudo）"
echo "======================================"
echo ""

# 检查 1：查看当前 macOS 版本
echo "检查 1：macOS 版本"
echo "--------------------------------------"
sw_vers
echo ""

# 检查 2：检查终端应用路径
echo "检查 2：终端应用路径"
echo "--------------------------------------"
which Terminal
ls -la /Applications/Utilities/Terminal.app
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

# 检查 4：检查当前用户
echo "检查 4：当前用户"
echo "--------------------------------------"
whoami
echo ""

# 检查 5：检查进程
echo "检查 5：相关进程"
echo "--------------------------------------"
ps aux | grep -i "system events" | grep -v grep
echo ""

# 检查 6：检查权限文件
echo "检查 6：权限文件"
echo "--------------------------------------"
ls -la ~/Library/Preferences/com.apple.universalaccess.plist 2>&1
echo ""

echo "======================================"
echo "检查完成"
echo "======================================"