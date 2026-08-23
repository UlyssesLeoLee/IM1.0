# 受入試験 / UAT
> 工程范围: No.90-95
> 阶段目的: UAT Plan→Spec→Exec→业务场景→判定→検収
> 出口 GATE: **検収**

## 文档清单
| 工程 | 文档 | 略称 | 责任方 | 文件 |
|---|---|---|---|---|
| 90 | UAT 計画書 / 验收测试计划 | UAT Plan | QA + 业务側(主导) | [90-uat-plan.md](90-uat-plan.md) |
| 91 | UAT 仕様書 / 验收测试规格 | UAT | 业务用户 + QA | [91-uat-specification.md](91-uat-specification.md) |
| 92 | UAT 結果報告書 / 验收测试结果 | UAT | 业务用户(主导) + QA | [92-uat-result-report.md](92-uat-result-report.md) |
| 93 | 業務シナリオ試験結果 / 业务场景综合结果 | — | 业务側 + QA | [93-business-scenario-report.md](93-business-scenario-report.md) |
| 94 | 受入判定書 / 验收判定 | — | 客户 / 业务代表 + PM | [94-acceptance-decision.md](94-acceptance-decision.md) |
| 95 | 検収書 / 验收书 | Acceptance | 客户 / 业务代表 + 经营 | [95-acceptance-letter.md](95-acceptance-letter.md) |

## 使用建议
1. 进入本 Phase 前,确认上游 GATE(検収 的上游)已通过。
2. 按 No. 顺序逐项填写对应文档;不必强求一次写满,可迭代。
3. 完成本 Phase 全部文档后,申请 検収 评审;通过后才能进入下一 Phase。
4. 残留问题 / 范围变更走 `docs/templates/15-management/136-scope-change-record.md`。
