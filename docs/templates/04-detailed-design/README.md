# 詳細設計 / Detailed Design
> 工程范围: No.42-52
> 阶段目的: 程序结构→模块→类→逻辑→API/DB/SQL/Batch/错误/日志详细 + DD Review
> 出口 GATE: **DD Review**

## 文档清单
| 工程 | 文档 | 略称 | 责任方 | 文件 |
|---|---|---|---|---|
| 42 | プログラム構造図 / 程序结构图 | — | Tech Lead | [42-program-structure.md](42-program-structure.md) |
| 43 | モジュール設計書 / 模块设计书 | — | Tech Lead + 開発者 | [43-module-design.md](43-module-design.md) |
| 44 | クラス図 (UML) / 类图 | — | 開発者 | [44-class-diagram.md](44-class-diagram.md) |
| 45 | ロジック仕様 / 逻辑规格 | — | 開発者 | [45-logic-specification.md](45-logic-specification.md) |
| 46 | API 詳細仕様 / API 详细规格 | — | Tech Lead + 開発者 | [46-api-detailed-spec.md](46-api-detailed-spec.md) |
| 47 | DB 詳細設計書 / 数据库详细设计 | — | DB 設計者 | [47-db-detailed-design.md](47-db-detailed-design.md) |
| 48 | SQL 設計 + レビュー記録 / SQL 设计与评审记录 | — | DBA + 開発者 | [48-sql-design-and-review.md](48-sql-design-and-review.md) |
| 49 | バッチ詳細仕様 / 批处理详细规格 | — | 開発者 + SRE | [49-batch-detailed-spec.md](49-batch-detailed-spec.md) |
| 50 | エラー処理方針書 / 错误处理方针 | — | Tech Lead | [50-error-handling-policy.md](50-error-handling-policy.md) |
| 51 | ログ設計書 / 日志设计 | — | Tech Lead + SRE | [51-log-design.md](51-log-design.md) |
| 52 | 詳細設計レビュー結果 / 详细设计评审记录 | DD Review | Tech Lead + 干系人 | [52-dd-review-record.md](52-dd-review-record.md) |

## 使用建议
1. 进入本 Phase 前,确认上游 GATE(DD Review 的上游)已通过。
2. 按 No. 顺序逐项填写对应文档;不必强求一次写满,可迭代。
3. 完成本 Phase 全部文档后,申请 DD Review 评审;通过后才能进入下一 Phase。
4. 残留问题 / 范围变更走 `docs/templates/15-management/136-scope-change-record.md`。
