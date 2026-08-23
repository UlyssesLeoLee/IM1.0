# 単体試験 / Unit Test
> 工程范围: No.59-65
> 阶段目的: UT Plan→Spec→Review→Exec→Bug→Retest→Approval
> 出口 GATE: **UT 完了**

## 文档清单
| 工程 | 文档 | 略称 | 责任方 | 文件 |
|---|---|---|---|---|
| 59 | UT 計画書 / 单元测试计划 | UT Plan | QA + Tech Lead | [59-ut-plan.md](59-ut-plan.md) |
| 60 | UT 仕様書 / 单元测试规格 | UT | 開発者 + QA | [60-ut-specification.md](60-ut-specification.md) |
| 61 | UT レビュー結果 / 单元测试评审记录 | UT Review | QA + Tech Lead | [61-ut-review-record.md](61-ut-review-record.md) |
| 62 | UT 結果報告書 / 单元测试结果 | UT | QA + 開発者 | [62-ut-execution-report.md](62-ut-execution-report.md) |
| 63 | バグ票 / Bug 票 | Bug Fix | QA / 開発者 | [63-bug-ticket.md](63-bug-ticket.md) |
| 64 | Retest 結果 / 回归 / 再测记录 | Retest | QA | [64-retest-record.md](64-retest-record.md) |
| 65 | UT 完了承認 / 单元测试完成批准 | — | PM + Tech Lead + QA | [65-ut-completion-approval.md](65-ut-completion-approval.md) |

## 使用建议
1. 进入本 Phase 前,确认上游 GATE(UT 完了 的上游)已通过。
2. 按 No. 顺序逐项填写对应文档;不必强求一次写满,可迭代。
3. 完成本 Phase 全部文档后,申请 UT 完了 评审;通过后才能进入下一 Phase。
4. 残留问题 / 范围变更走 `docs/templates/15-management/136-scope-change-record.md`。
