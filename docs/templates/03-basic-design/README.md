# 基本設計 / Basic Design
> 工程范围: No.22-41
> 阶段目的: 方式→架构→功能/UI/帳票/API/IF/DB/ER/Batch/权限/安全/Infra/NW/运用/监控/备份/迁移设计 + BD Review
> 出口 GATE: **BD Review**

## 文档清单
| 工程 | 文档 | 略称 | 责任方 | 文件 |
|---|---|---|---|---|
| 22 | システム方式書 / 系统方式设计书 | SA | SA | [22-system-architecture.md](22-system-architecture.md) |
| 23 | ソフトウェア方式書 / 软件方式设计书 | — | SA + Tech Lead | [23-software-architecture.md](23-software-architecture.md) |
| 24 | アーキテクチャ図 + ADR / 架构图 + 架构决策记录 | Architecture | SA | [24-architecture-and-adr.md](24-architecture-and-adr.md) |
| 25 | 機能設計書 / 功能设计书 | — | Tech Lead + 開発者 | [25-functional-design.md](25-functional-design.md) |
| 26 | 画面遷移図 + ワイヤーフレーム / 画面设计(画面遷移 + 線框圖) | UI | UI / UX デザイナー | [26-ui-design.md](26-ui-design.md) |
| 27 | 帳票定義書 / 报表设计书 | — | BA + 開発者 | [27-report-design.md](27-report-design.md) |
| 28 | API 仕様書(OpenAPI / proto) / API 规格书 | API | Tech Lead + 開発者 | [28-api-specification.md](28-api-specification.md) |
| 29 | 外部 IF 詳細設計書 / 外部接口详细设计 | IF | BA + 開発者 + 外部担当 | [29-if-detailed-design.md](29-if-detailed-design.md) |
| 30 | DB 基本設計書 / 数据库基本设计 | DB | DB 設計者 + Tech Lead | [30-db-basic-design.md](30-db-basic-design.md) |
| 31 | ER 図 / ER 图(概念 / 逻辑) | ER | DB 設計者 + BA | [31-er-diagram.md](31-er-diagram.md) |
| 32 | バッチ設計書 / 批处理设计 | Batch | Tech Lead + 運用リード | [32-batch-design.md](32-batch-design.md) |
| 33 | 権限マトリクス / 权限矩阵 | — | セキュリティリード + BA | [33-permission-matrix.md](33-permission-matrix.md) |
| 34 | セキュリティ設計書 / 安全设计 | — | セキュリティリード + SA | [34-security-design.md](34-security-design.md) |
| 35 | インフラ基本設計書 / 基础设施基本设计 | Infra | SRE / Infra リード | [35-infra-basic-design.md](35-infra-basic-design.md) |
| 36 | ネットワーク基本設計書 / 网络基本设计 | NW | Network / SRE | [36-network-basic-design.md](36-network-basic-design.md) |
| 37 | 運用設計書 / 运维设计 | — | SRE / 運用リード | [37-operation-design.md](37-operation-design.md) |
| 38 | 監視設計書 / 监控设计 | — | SRE | [38-monitoring-design.md](38-monitoring-design.md) |
| 39 | バックアップ設計書 / 备份设计 | — | SRE / DBA | [39-backup-design.md](39-backup-design.md) |
| 40 | 移行設計書 / 迁移设计 | — | Tech Lead + 運用リード | [40-migration-design.md](40-migration-design.md) |
| 41 | 基本設計レビュー結果 / 基本设计评审记录 | BD Review | SA + 干系人 | [41-bd-review-record.md](41-bd-review-record.md) |

## 使用建议
1. 进入本 Phase 前,确认上游 GATE(BD Review 的上游)已通过。
2. 按 No. 顺序逐项填写对应文档;不必强求一次写满,可迭代。
3. 完成本 Phase 全部文档后,申请 BD Review 评审;通过后才能进入下一 Phase。
4. 残留问题 / 范围变更走 `docs/templates/15-management/136-scope-change-record.md`。
