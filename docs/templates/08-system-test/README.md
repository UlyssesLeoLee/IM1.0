# システム試験 / System Test
> 工程范围: No.76-89
> 阶段目的: ST Plan→Spec→功能/场景/性能/负载/压力/安全/故障/恢复/B-R/可用性/运维 + Approval
> 出口 GATE: **ST 完了**

## 文档清单
| 工程 | 文档 | 略称 | 责任方 | 文件 |
|---|---|---|---|---|
| 76 | ST 計画書 / 系统测试计划 | ST Plan | QA + SRE + Tech Lead | [76-st-plan.md](76-st-plan.md) |
| 77 | ST 仕様書 / 系统测试规格 | ST | QA + SRE + Tech Lead | [77-st-specification.md](77-st-specification.md) |
| 78 | 機能試験結果 / 功能(端到端)系统测试结果 | — | QA | [78-functional-st-report.md](78-functional-st-report.md) |
| 79 | シナリオ試験結果 / 业务场景测试结果 | — | QA + 业务側 | [79-scenario-st-report.md](79-scenario-st-report.md) |
| 80 | 性能試験結果 (PT) / 性能测试结果 | PT | SRE + QA | [80-pt-report.md](80-pt-report.md) |
| 81 | 負荷試験結果 / 负载测试结果 | Load Test | SRE + QA | [81-load-test-report.md](81-load-test-report.md) |
| 82 | ストレス試験結果 / 压力测试结果 | Stress | SRE | [82-stress-test-report.md](82-stress-test-report.md) |
| 83 | セキュリティ試験結果 / 安全测试结果 | Security | セキュリティ + 外部第三方 | [83-security-st-report.md](83-security-st-report.md) |
| 84 | 障害試験結果 / 故障注入测试 | — | SRE + Tech Lead | [84-failure-injection-report.md](84-failure-injection-report.md) |
| 85 | 復旧試験結果 / 恢复测试 | Recovery | SRE + DBA | [85-recovery-test-report.md](85-recovery-test-report.md) |
| 86 | バックアップ・リストア試験結果 (B/R) / 备份恢复测试 | B/R | SRE + DBA | [86-backup-restore-report.md](86-backup-restore-report.md) |
| 87 | 可用性試験結果 / 可用性测试 | — | SRE | [87-availability-test-report.md](87-availability-test-report.md) |
| 88 | 運用試験結果 (OT) / 运维测试 | OT | SRE + 運用チーム | [88-ot-report.md](88-ot-report.md) |
| 89 | ST 完了承認 / 系统测试完成批准 | — | PMO + Tech Lead + QA + SRE | [89-st-completion-approval.md](89-st-completion-approval.md) |

## 使用建议
1. 进入本 Phase 前,确认上游 GATE(ST 完了 的上游)已通过。
2. 按 No. 顺序逐项填写对应文档;不必强求一次写满,可迭代。
3. 完成本 Phase 全部文档后,申请 ST 完了 评审;通过后才能进入下一 Phase。
4. 残留问题 / 范围变更走 `docs/templates/15-management/136-scope-change-record.md`。
