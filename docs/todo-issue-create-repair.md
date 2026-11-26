# Todo: 恢复未完成的 Issue 创建

- **Status**: 验证成功
- **Scope**: DM-03 创建去重与中断修复；独立使用现有 Gitea Adapter。
- **Baseline**: 无 Git 元数据；现有 `TaskService::create` 遇到相同 dedup_key 直接返回，可能留下空 issue_ref 或缺标签。

## Behavior

- Input: 同一 dedup_key 的创建请求，已有 staging Issue 或标签投影未完成的 Issue。
- Output: 复用原 Issue，CAS 补齐 issue_ref，增量补齐机器标签，返回原 task_id。
- Constraints/errors: 不重发 POST 创建，不覆盖人工标签；已绑定其他 Issue 的引用明确报错；保留 CAS 冲突，不猜测成功。

## Test first

- 用可注入 GiteaTransport 模拟创建成功但响应丢失、CAS 更新失败和标签失败。
- 重复请求应修复原 Issue；已完成的请求保持幂等；保留人工标签。
- Expected RED: 当前重复请求直接返回空 issue_ref；首次标签使用 PUT 会覆盖人工标签。

## Implementation

提取创建完成步骤供首次与重复请求使用；仅缺少引用时写正文，标签采用增量添加。并发创建的服务端唯一性不在本修复中承诺。

## Verification

- Focused: `cargo test -p devmesh-agent --test issue_create --offline`
- Regression: Gitea、Worker、MCP 测试，Rust fmt/Clippy。

## Done

- [x] 测试先写且观察 RED。
- [x] 修复与聚焦/相关回归通过。
- [x] Evidence 已记录。

## Evidence 2026-11-26

- RED: 创建丢失响应后重复调用返回空 issue_ref；PUT 标签丢失人工标签。
- GREEN: issue_create 2 项、Gitea 10 项和 Worker 3 项测试通过；Clippy 通过。
- Changed files: devmesh-agent/tests/issue_create.rs；devmesh-agent/src/gitea/issue.rs
- Limitation: 真实网络/浏览器验收尚未执行；不表示 DM-03/09 整节点完成。
