# 運用 / Operations
> 工程范围: No.109-117
> 阶段目的: 引継ぎ→Monitoring→Job→Backup→Capacity→Incident→障害→Problem→Support
> 出口 GATE: **SLA 持续**

## 文档清单
| 工程 | 文档 | 略称 | 责任方 | 文件 |
|---|---|---|---|---|
| 109 | 運用引継ぎチェックリスト / 运维交接清单 | Handover | PM + SRE Lead | [109-handover-checklist.md](109-handover-checklist.md) |
| 110 | 監視ダッシュボード構成 / 监控仪表盘 | Monitoring | SRE | [110-monitoring-dashboard.md](110-monitoring-dashboard.md) |
| 111 | ジョブ運用 Runbook / Job 运维 Runbook | Job | SRE | [111-job-runbook.md](111-job-runbook.md) |
| 112 | バックアップ履歴 / 备份历史 | Backup | SRE + DBA | [112-backup-history.md](112-backup-history.md) |
| 113 | キャパシティレポート / 容量报告 | Capacity | SRE | [113-capacity-report.md](113-capacity-report.md) |
| 114 | インシデントレポート / 事件报告 | Incident | SRE + 应急响应人员 | [114-incident-report.md](114-incident-report.md) |
| 115 | 障害記録 / 故障记录 | — | SRE | [115-failure-record.md](115-failure-record.md) |
| 116 | RCA + 恒久対策 / 根因分析与永久对策 | Problem | SRE + Tech Lead | [116-rca-and-permanent-fix.md](116-rca-and-permanent-fix.md) |
| 117 | 問い合わせ対応記録 / 咨询 / 工单记录 | Support | サポート / CS | [117-support-ticket.md](117-support-ticket.md) |

## 使用建议
1. 进入本 Phase 前,确认上游 GATE(SLA 持续 的上游)已通过。
2. 按 No. 顺序逐项填写对应文档;不必强求一次写满,可迭代。
3. 完成本 Phase 全部文档后,申请 SLA 持续 评审;通过后才能进入下一 Phase。
4. 残留问题 / 范围变更走 `docs/templates/15-management/136-scope-change-record.md`。
