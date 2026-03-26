# ADR-014: 前后端分离 — 三进程架构

## 状态

已决策（2026-03-26），M5 交互层时落地。

## 背景

当前 Dashboard 和未来的 Chat 客户端耦合在同一个 Tauri 应用中（单进程）。
两者的 UX 模式完全不同：

- Chat：流式对话、紧凑窗口、高频交互
- Dashboard：表格/图表/表单、全屏管理后台、低频访问

耦合会导致：
- 改 Chat 影响 Dashboard，反之亦然
- 无法独立部署（服务器上只需要 Core，不需要 GUI）
- 技术栈选型互相制约

## 决策

**三进程架构，通过 HTTP/WS 通信：**

```
进程 1: UMMS Core Service（Rust, Axum）
├── HTTP REST API
├── WebSocket（流式对话）
├── MCP Server（Agent 协议）
├── umms-storage / retriever / encoder / consolidation
└── 可 headless 运行，无 GUI 依赖

进程 2: Dashboard（Vue 3 + Naive UI）
├── 独立前端应用
├── 调用 Core Service HTTP API
├── 可部署到任意静态托管
└── 管理/监控/配置中心

进程 3: Chat Client（独立前端）
├── 独立前端应用
├── 调用 Core Service HTTP/WS API
├── 可选 Tauri 包装为桌面应用
└── 用户日常交互入口
```

## 影响

### src-tauri 角色变化

- 当前：Tauri 是唯一入口，内嵌 Axum + Dashboard
- 变更后：Tauri 变为 Chat Client 的可选桌面包装
- `umms-api` crate 回归为独立可运行的 Axum 服务（`cargo run -p umms-api`）

### 部署灵活性

| 场景 | 部署方式 |
|------|---------|
| 本地开发 | 3 个进程都在本地 |
| 个人桌面 | Core 后台常驻 + Tauri Chat 窗口 + 浏览器开 Dashboard |
| 服务器 | 只跑 Core（headless），前端远程访问 |
| 团队共享 | Core 部署一份，多人各自连接 Chat/Dashboard |

### API 契约不变

Dashboard 和 Chat 都通过同一套 HTTP/WS API 与 Core 通信，
`umms-api` 的路由和响应类型是唯一契约。三个进程的代码变更互不影响。

## 实施时机

M5 交互层开发时落地。具体步骤：
1. 将 `umms-api` 改为可独立启动的 binary（`main.rs`），不依赖 Tauri
2. Dashboard 改为纯静态前端，连接 Core Service 的 HTTP 地址（可配置）
3. 新建 Chat Client 前端项目
4. src-tauri 保留为 Chat Client 的 Tauri 包装（可选）
5. 移除 Dashboard 对 Tauri IPC 的依赖（已有 HTTP fallback，改为默认）
