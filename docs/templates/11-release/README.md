# リリース / Release
> 工程范围: No.102-108
> 阶段目的: Release Plan→Go/No-Go→本番环境→Deploy→Smoke→Go-Live→Hypercare
> 出口 GATE: **Go-Live**

## 文档清单
| 工程 | 文档 | 略称 | 责任方 | 文件 |
|---|---|---|---|---|
| 102 | リリース計画書 / 发布计划 | Release Plan | PM + SRE + Tech Lead | [102-release-plan.md](102-release-plan.md) |
| 103 | Go / No-Go 決定書 / 发布决策 | Go/No-Go | PMO + Sponsor + Tech Lead | [103-go-no-go-decision.md](103-go-no-go-decision.md) |
| 104 | 本番環境構築記録 / 生产环境搭建 | Production | SRE | [104-prod-env-build-record.md](104-prod-env-build-record.md) |
| 105 | 本番デプロイ記録 / 生产部署 | Deploy | SRE + Tech Lead | [105-prod-deploy-record.md](105-prod-deploy-record.md) |
| 106 | 稼働確認 (Smoke Test) 結果 / 冒烟测试 | Smoke Test | SRE + QA | [106-smoke-test-report.md](106-smoke-test-report.md) |
| 107 | Go-Live 宣言 / 上线宣言 | Go-Live | Sponsor + PM | [107-go-live-declaration.md](107-go-live-declaration.md) |
| 108 | Hypercare レポート / 初期密集护航 | Hypercare | SRE + Dev | [108-hypercare-report.md](108-hypercare-report.md) |

## 使用建议
1. 进入本 Phase 前,确认上游 GATE(Go-Live 的上游)已通过。
2. 按 No. 顺序逐项填写对应文档;不必强求一次写满,可迭代。
3. 完成本 Phase 全部文档后,申请 Go-Live 评审;通过后才能进入下一 Phase。
4. 残留问题 / 范围变更走 `docs/templates/15-management/136-scope-change-record.md`。
