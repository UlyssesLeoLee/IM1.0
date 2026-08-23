# 結合試験 / Integration Test
> 工程范围: No.66-75
> 阶段目的: IT Plan→Spec→Env→ITa→ITb→API→DB→外部 IF→Issue→Regression
> 出口 GATE: **Regression OK**

## 文档清单
| 工程 | 文档 | 略称 | 责任方 | 文件 |
|---|---|---|---|---|
| 66 | IT 計画書 / 集成测试计划 | IT Plan | QA + Tech Lead | [66-it-plan.md](66-it-plan.md) |
| 67 | IT 仕様書 / 集成测试规格 | IT | QA + 開発者 | [67-it-specification.md](67-it-specification.md) |
| 68 | IT 環境構築手順 / 集成测试环境搭建 | — | DevOps + QA | [68-it-env-setup.md](68-it-env-setup.md) |
| 69 | 内部結合試験結果 (ITa) / 内部集成测试结果 | ITa | QA + 開発者 | [69-ita-report.md](69-ita-report.md) |
| 70 | 外部結合試験結果 (ITb) / 外部集成测试结果 | ITb | QA + 外部担当 | [70-itb-report.md](70-itb-report.md) |
| 71 | API 結合試験結果 / API 集成测试结果 | — | QA + Tech Lead | [71-api-it-report.md](71-api-it-report.md) |
| 72 | DB 結合試験結果 / 数据库集成测试结果 | — | DBA + QA | [72-db-it-report.md](72-db-it-report.md) |
| 73 | 外部システム連携試験結果 / 外部系统联动测试结果 | — | QA + 外部担当 | [73-external-if-it-report.md](73-external-if-it-report.md) |
| 74 | 障害・不具合対応記録 / 问题处理记录 | — | QA + 開発者 | [74-issue-handling-record.md](74-issue-handling-record.md) |
| 75 | 回帰試験結果 / 回归测试结果 | Regression | QA | [75-regression-report.md](75-regression-report.md) |

## 使用建议
1. 进入本 Phase 前,确认上游 GATE(Regression OK 的上游)已通过。
2. 按 No. 顺序逐项填写对应文档;不必强求一次写满,可迭代。
3. 完成本 Phase 全部文档后,申请 Regression OK 评审;通过后才能进入下一 Phase。
4. 残留问题 / 范围变更走 `docs/templates/15-management/136-scope-change-record.md`。
