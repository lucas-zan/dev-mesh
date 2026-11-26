# Runtime 验证范围

| Runtime | 适配边界 | 本地已测 | 真实环境待测 |
| --- | --- | --- | --- |
| Codex 0.172.0 | App Server JSON-RPC | 事件/命令映射、审批/输入、独立子进程协议、测试替身开发闭环 | 实际模型任务、Steer、审批、中断和恢复 |
| Pi 1.6.0 | RPC + Extension | epoch/状态控制、工具接口、公共事件 Schema | 实际 CLI 安装、模型任务、UI 请求、队列和恢复 |
| Pi Durable 1.6.0 | SDK + TypeScript Host | 实际 SDK/SQLite 持久会话、Rust→Host 管道、停止、超时配置 | 实际模型任务、工具审批、活动任务控制与恢复 |

所有版本均由安装器固定。doctor 的 installed、handshake、capabilities 和 native_e2e 是不同层次；协议测试通过或 SDK 可启动，不表示实际模型任务验收成功。
