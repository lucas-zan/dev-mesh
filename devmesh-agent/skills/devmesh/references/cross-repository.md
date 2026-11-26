# 跨角色与跨仓

先用 `repository_list` 从 Token 可见的所有仓库中找到目标仓库，用 Gitea 查询已有需求或缺陷。以来源 Issue、目标仓库、类型、规范业务标识构造稳定 dedup_key；存在相同任务时更新评论/证据，不再创建。

缺 API：frontend 创建 backend Issue，并把自己的任务标为依赖阻塞。QA 发现缺陷：关联被测精确 SHA 和失败验收项，选择已有任务返工或独立 Bug 作为修复主线，避免两套重复修复。

依赖携带完整实例、仓库数字 ID、Issue 编号；文档和 API 契约引用 commit+path 或带 SHA256 的版本产物。不能依赖“最新”URL。创建前检测循环，QA 可依赖 PR candidate，不必等待开发 Issue done。未配置/未授权目标只报告缺口，不擅自操作其他仓库。
