# Todo: Web 快照与 SSE 连续刷新

- **Status**: 验证成功
- **Scope**: DM-10 Web 读取快照并从游标订阅 feed，修复刷新期间事件被丢弃。

## Behavior

- Input: 首次项目快照 as_of_cursor、SSE lastEventId、异步快照刷新。
- Output: 从快照游标订阅；刷新期间新事件触发后续刷新；成功后才推进已应用游标。
- Constraints/errors: 使用 BigInt 保留大序号；刷新失败保留游标并显示错误；切换选择不重建全局连接；卸载关闭连接。

## Test first

- 在第一轮刷新尚未结束时送入更新，必须执行第二轮；重复事件不重复刷新；失败不推进游标；关闭后不刷新。
- Expected RED: 当前没有订阅协调器，main.tsx running=true 直接丢弃新事件。

## Implementation

窄 SSE 订阅函数接受刷新回调与 EventSource 工厂；串行合并通知，不增加后台轮询或重试层。Workspace 捕获初始快照游标，稳定订阅使用最新选择回调。

## Verification

- Web 测试、TypeScript/Vite 构建；实际浏览器联调另行记录。

## Done

- [x] RED 已观察。
- [x] 测试与构建通过。

## Evidence 2026-11-26

- RED: 缺 feed.ts；原刷新逻辑 running=true 时丢弃后续通知。
- GREEN: Web 测试及 TS/Vite build 通过；完整精度游标、并发通知、失败与重连均有测试。
- Limitation: 本机真实模型和浏览器交互未验收；不据此标记总计划完成。
