# Todo: 更改 Relay/Web 默认端口

- **Status**: 验证成功
- **Scope**: 默认端口从 8080 调整为 17843；命令行仍可显式 --listen 覆盖。

## Behavior / Test first

Relay 默认监听 127.0.0.1:17843；Docker 监听和端口映射、Agent 示例与 README 使用同一端口。先测试默认和显式覆盖，再改实现。测试不要求实际绑定端口。

## Verification

Relay main 单元测试、cargo fmt、构建；配置和文档引用一致性检查。不修改已有本机安装配置或历史发布包。

## Done

- [x] RED 已观察。
- [x] 改动和聚焦验证通过。

## Evidence

RED: 缺少 listen_address 默认端口入口。GREEN: 默认/覆盖单元测试、Relay 构建和 --help 输出通过，部署/示例/文档端口一致。当前环境未实际监听验证端口空闲情况。
