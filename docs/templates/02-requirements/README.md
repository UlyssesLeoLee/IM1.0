# 要件定義 / Requirements
> 工程范围: No.10-21
> 阶段目的: UR→BR→SR→FR/NFR/Data/IF/Security/Ops/Migration 要件 + Review + Baseline
> 出口 GATE: **要件 Baseline**

## 文档清单
| 工程 | 文档 | 略称 | 责任方 | 文件 |
|---|---|---|---|---|
| 10 | ユーザー要求マトリクス / 用户要求矩阵 | UR | BA | [10-ur-matrix.md](10-ur-matrix.md) |
| 11 | 業務要件定義書 / 业务需求定义书 | BR | BA + 業務側 | [11-business-requirements.md](11-business-requirements.md) |
| 12 | システム要件定義書 / 系统需求定义书 | SR | SA + BA | [12-system-requirements.md](12-system-requirements.md) |
| 13 | 機能要件一覧 / 功能需求清单 | FR | BA + 開発リード | [13-functional-requirements.md](13-functional-requirements.md) |
| 14 | NFR マトリクス / 非功能需求矩阵 | NFR | SA + 技術リード | [14-nfr-matrix.md](14-nfr-matrix.md) |
| 15 | データ要件定義書 / 数据需求定义书 | — | データアーキテクト + BA | [15-data-requirements.md](15-data-requirements.md) |
| 16 | 外部 IF 要件一覧 / 外部接口需求清单 | IF | BA + 外部システム担当 | [16-if-requirements.md](16-if-requirements.md) |
| 17 | セキュリティ要件定義書 / 安全需求定义书 | — | セキュリティリード + SA | [17-security-requirements.md](17-security-requirements.md) |
| 18 | 運用要件一覧 / 运维需求清单 | — | SRE / 運用リード | [18-operation-requirements.md](18-operation-requirements.md) |
| 19 | 移行要件一覧 / 迁移需求清单 | — | 運用リード + BA | [19-migration-requirements.md](19-migration-requirements.md) |
| 20 | 要件レビュー結果 / 需求评审记录 | RD Review | PM + 各干系人 | [20-rd-review-record.md](20-rd-review-record.md) |
| 21 | 要件 Baseline 登録票 / 需求基线登记表 | Baseline | PMO | [21-baseline-registration.md](21-baseline-registration.md) |

## 使用建议
1. 进入本 Phase 前,确认上游 GATE(要件 Baseline 的上游)已通过。
2. 按 No. 顺序逐项填写对应文档;不必强求一次写满,可迭代。
3. 完成本 Phase 全部文档后,申请 要件 Baseline 评审;通过后才能进入下一 Phase。
4. 残留问题 / 范围变更走 `docs/templates/15-management/136-scope-change-record.md`。
