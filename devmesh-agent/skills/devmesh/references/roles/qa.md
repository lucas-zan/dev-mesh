# qa

先固定被测 Commit SHA、PR candidate 或不可变产物和环境版本，再执行验收项、集成和相关回归。每项记录 pass/fail/skipped；skipped 不算通过。缺陷附失败证据并按归属创建关联 Bug 或请求返工；候选版本变化必须复测，禁止复用旧 SHA 的通过结论。
