# DevMesh V1.1 研发设计包

- **DevMesh-V1.1.md**：完整替换版产品与技术规格（保留 Web UI 原图），不是在旧版上追加互相冲突的规则。
- **review-resolution.md**：截图评审逐项闭环、追加发布验收测试。
- **protocol/contract.md**：跨四仓协议实现合同，WSS / IPC / REST / SSE / 错误 / 版本。
- **protocol/schemas/**：Issue 元数据、WSS/Event/Command、Rust/TS IPC 的 JSON Schema。
- **protocol/fixtures/**：正常与异常结构样例；`python protocol/validate_fixtures.py` 可复查（需 jsonschema）。
- **assets/web-workspace.png**：原 Web UI 参考图。实际产品名改为 DevMesh。

代码仓库：devmesh-agent、devmesh-relay（含 Web）、devmesh-app、devmesh-protocol。云端仍仅一个业务服务与 PostgreSQL。将本包作为 V1.1 需求与协议基线；V1.0 不再作为实现规则。D0 前置验证及 D1 正式 SDK/接口生成尚未执行。
