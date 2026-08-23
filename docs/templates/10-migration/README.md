# 移行 / Migration
> 工程范围: No.96-101
> 阶段目的: 移行計画→手順→リハ→Data→切替→結果
> 出口 GATE: **移行成功**

## 文档清单
| 工程 | 文档 | 略称 | 责任方 | 文件 |
|---|---|---|---|---|
| 96 | 移行計画書 / 迁移计划 | Migration Plan | Tech Lead + 運用リード | [96-migration-plan.md](96-migration-plan.md) |
| 97 | 移行手順書 / 迁移步骤 | — | SRE + DBA + 開発者 | [97-migration-procedure.md](97-migration-procedure.md) |
| 98 | 移行リハ結果 / 迁移演练结果 | Rehearsal | SRE + 運用リード | [98-migration-rehearsal-report.md](98-migration-rehearsal-report.md) |
| 99 | データ移行結果 / 数据迁移结果 | Data Migration | DBA + SRE | [99-data-migration-report.md](99-data-migration-report.md) |
| 100 | システム移行記録 / 系统切替记录 | — | SRE + Tech Lead | [100-system-cutover-record.md](100-system-cutover-record.md) |
| 101 | 移行結果報告書 / 迁移结果报告 | — | PM + SRE + 客户 | [101-migration-result-report.md](101-migration-result-report.md) |

## 使用建议
1. 进入本 Phase 前,确认上游 GATE(移行成功 的上游)已通过。
2. 按 No. 顺序逐项填写对应文档;不必强求一次写满,可迭代。
3. 完成本 Phase 全部文档后,申请 移行成功 评审;通过后才能进入下一 Phase。
4. 残留问题 / 范围变更走 `docs/templates/15-management/136-scope-change-record.md`。
