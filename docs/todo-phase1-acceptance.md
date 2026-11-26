# Todo: 第一期真实环境验收入口

- **Status**: 待测试验证
- **Scope**: 汇总本地完整流程、三 Runtime、Relay/Web 与一键安装的实际证据。

## Behavior

- 无凭据时仍可完成 Git、SQLite、子进程、内存传输和安装包验证。
- 真实环境入口读取配置和凭据引用，记录连接/运行实际结果，不输出秘密；缺少条件报告 NOT_RUN，不能当 PASS。
- 提供专用仓库/Issue 的手动验收步骤及机器可读报告，区分 doctor 握手与真实模型完成。

## Test first

- 纯报告归并验证：未执行/失败项不通过，全部已执行通过才成功；不把空报告记为通过。
- 源码缺少真实验收 runner 时应失败。

## Implementation

复用现有 CLI 与脚本，不额外创建凭据或后台进程。已禁止端口/进程操作的环境保留明确限制，真实 Token/Key 最后使用。

## Verification

聚焦 runner 测试、make verify、离线 e2e、发布包和校验和。

## Done

- [x] 开发与本地验证完成。
- [ ] 真实环境验收完成，或明确记录未执行原因。

## Evidence 2026-11-26T01:44:54.719948+00:00

`test_live_validation.py` 4 项通过；`validate_live.py` 当前返回退出码 2，TCP/Unix/进程检查 FAIL，doctor NOT_RUN。已发布真实验收手册，不宣称模型或部署验收完成。
