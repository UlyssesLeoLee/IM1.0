# IM1.0 Workflow Templates

> 本目录是 `docs/Workflow.md` 中 150 个工程活动的配套文档模板。
> 每个 .md 文件 = 1 个工程活动的主输出文档模板;按 16 个 Phase 子目录组织。

## 目录

- [超上流 / Upstream (01-09)](./01-upstream/) — 经营要求→系统化企画→立上げ→To-Be 业务设计;GATE: **PJ 立上げ**
- [要件定義 / Requirements (10-21)](./02-requirements/) — UR→BR→SR→FR/NFR/Data/IF/Security/Ops/Migration 要件 + Review + Baseline;GATE: **要件 Baseline**
- [基本設計 / Basic Design (22-41)](./03-basic-design/) — 方式→架构→功能/UI/帳票/API/IF/DB/ER/Batch/权限/安全/Infra/NW/运用/监控/备份/迁移设计 + BD Review;GATE: **BD Review**
- [詳細設計 / Detailed Design (42-52)](./04-detailed-design/) — 程序结构→模块→类→逻辑→API/DB/SQL/Batch/错误/日志详细 + DD Review;GATE: **DD Review**
- [実装 / Implementation (53-58)](./05-implementation/) — 开发环境→编码规范→SAST→CR→Build→CI;GATE: **Build + CR**
- [単体試験 / Unit Test (59-65)](./06-unit-test/) — UT Plan→Spec→Review→Exec→Bug→Retest→Approval;GATE: **UT 完了**
- [結合試験 / Integration Test (66-75)](./07-integration-test/) — IT Plan→Spec→Env→ITa→ITb→API→DB→外部 IF→Issue→Regression;GATE: **Regression OK**
- [システム試験 / System Test (76-89)](./08-system-test/) — ST Plan→Spec→功能/场景/性能/负载/压力/安全/故障/恢复/B-R/可用性/运维 + Approval;GATE: **ST 完了**
- [受入試験 / UAT (90-95)](./09-uat/) — UAT Plan→Spec→Exec→业务场景→判定→検収;GATE: **検収**
- [移行 / Migration (96-101)](./10-migration/) — 移行計画→手順→リハ→Data→切替→結果;GATE: **移行成功**
- [リリース / Release (102-108)](./11-release/) — Release Plan→Go/No-Go→本番环境→Deploy→Smoke→Go-Live→Hypercare;GATE: **Go-Live**
- [運用 / Operations (109-117)](./12-operations/) — 引継ぎ→Monitoring→Job→Backup→Capacity→Incident→障害→Problem→Support;GATE: **SLA 持续**
- [保守 / Maintenance (118-126)](./13-maintenance/) — CR→Impact→Approval→CM→Patch→Vuln→Maintenance→Hotfix→Regression;GATE: **变更关闭**
- [品質管理 / Quality (127-130)](./14-quality/) — QA Plan→Review→Assessment→Audit;GATE: **QA 评价**
- [管理 / Management (131-144)](./15-management/) — PJ Plan→WBS→Progress→Issue→Risk→Change→CM→Deliverable→Review→Meeting→Effort→Cost→Scope→Baseline;GATE: **持续**
- [終結 / Closure (145-150)](./16-closure/) — 完了判定→引渡し→報告→振り返り→KT→Archive;GATE: **项目关闭**

## 全部模板(按工程编号)

| No | 阶段 | 文档(中文) | 略称 | 文件 |
|---|---|---|---|---|
| 01 | 01-upstream | 经营要求确认备忘 | — | [01-management-requirements-memo.md](./01-upstream/01-management-requirements-memo.md) |
| 02 | 01-upstream | 系统化构想书 | — | [02-system-concept.md](./01-upstream/02-system-concept.md) |
| 03 | 01-upstream | 系统化计划书 | — | [03-system-plan.md](./01-upstream/03-system-plan.md) |
| 04 | 01-upstream | 企划书 / 产品 PRD | — | [04-product-proposal.md](./01-upstream/04-product-proposal.md) |
| 05 | 01-upstream | 项目章程 + 启动会资料 | PJ立上げ | [05-project-charter.md](./01-upstream/05-project-charter.md) |
| 06 | 01-upstream | 现有业务流程图 | As-Is | [06-as-is-business-flow.md](./01-upstream/06-as-is-business-flow.md) |
| 07 | 01-upstream | 现有系统架构图 | As-Is | [07-as-is-system-architecture.md](./01-upstream/07-as-is-system-architecture.md) |
| 08 | 01-upstream | 课题一览 / 改善主题 | — | [08-issue-and-opportunity-list.md](./01-upstream/08-issue-and-opportunity-list.md) |
| 09 | 01-upstream | 目标态业务流程 | To-Be | [09-to-be-business-flow.md](./01-upstream/09-to-be-business-flow.md) |
| 10 | 02-requirements | 用户要求矩阵 | UR | [10-ur-matrix.md](./02-requirements/10-ur-matrix.md) |
| 11 | 02-requirements | 业务需求定义书 | BR | [11-business-requirements.md](./02-requirements/11-business-requirements.md) |
| 12 | 02-requirements | 系统需求定义书 | SR | [12-system-requirements.md](./02-requirements/12-system-requirements.md) |
| 13 | 02-requirements | 功能需求清单 | FR | [13-functional-requirements.md](./02-requirements/13-functional-requirements.md) |
| 14 | 02-requirements | 非功能需求矩阵 | NFR | [14-nfr-matrix.md](./02-requirements/14-nfr-matrix.md) |
| 15 | 02-requirements | 数据需求定义书 | — | [15-data-requirements.md](./02-requirements/15-data-requirements.md) |
| 16 | 02-requirements | 外部接口需求清单 | IF | [16-if-requirements.md](./02-requirements/16-if-requirements.md) |
| 17 | 02-requirements | 安全需求定义书 | — | [17-security-requirements.md](./02-requirements/17-security-requirements.md) |
| 18 | 02-requirements | 运维需求清单 | — | [18-operation-requirements.md](./02-requirements/18-operation-requirements.md) |
| 19 | 02-requirements | 迁移需求清单 | — | [19-migration-requirements.md](./02-requirements/19-migration-requirements.md) |
| 20 | 02-requirements | 需求评审记录 | RD Review | [20-rd-review-record.md](./02-requirements/20-rd-review-record.md) |
| 21 | 02-requirements | 需求基线登记表 | Baseline | [21-baseline-registration.md](./02-requirements/21-baseline-registration.md) |
| 22 | 03-basic-design | 系统方式设计书 | SA | [22-system-architecture.md](./03-basic-design/22-system-architecture.md) |
| 23 | 03-basic-design | 软件方式设计书 | — | [23-software-architecture.md](./03-basic-design/23-software-architecture.md) |
| 24 | 03-basic-design | 架构图 + 架构决策记录 | Architecture | [24-architecture-and-adr.md](./03-basic-design/24-architecture-and-adr.md) |
| 25 | 03-basic-design | 功能设计书 | — | [25-functional-design.md](./03-basic-design/25-functional-design.md) |
| 26 | 03-basic-design | 画面设计(画面遷移 + 線框圖) | UI | [26-ui-design.md](./03-basic-design/26-ui-design.md) |
| 27 | 03-basic-design | 报表设计书 | — | [27-report-design.md](./03-basic-design/27-report-design.md) |
| 28 | 03-basic-design | API 规格书 | API | [28-api-specification.md](./03-basic-design/28-api-specification.md) |
| 29 | 03-basic-design | 外部接口详细设计 | IF | [29-if-detailed-design.md](./03-basic-design/29-if-detailed-design.md) |
| 30 | 03-basic-design | 数据库基本设计 | DB | [30-db-basic-design.md](./03-basic-design/30-db-basic-design.md) |
| 31 | 03-basic-design | ER 图(概念 / 逻辑) | ER | [31-er-diagram.md](./03-basic-design/31-er-diagram.md) |
| 32 | 03-basic-design | 批处理设计 | Batch | [32-batch-design.md](./03-basic-design/32-batch-design.md) |
| 33 | 03-basic-design | 权限矩阵 | — | [33-permission-matrix.md](./03-basic-design/33-permission-matrix.md) |
| 34 | 03-basic-design | 安全设计 | — | [34-security-design.md](./03-basic-design/34-security-design.md) |
| 35 | 03-basic-design | 基础设施基本设计 | Infra | [35-infra-basic-design.md](./03-basic-design/35-infra-basic-design.md) |
| 36 | 03-basic-design | 网络基本设计 | NW | [36-network-basic-design.md](./03-basic-design/36-network-basic-design.md) |
| 37 | 03-basic-design | 运维设计 | — | [37-operation-design.md](./03-basic-design/37-operation-design.md) |
| 38 | 03-basic-design | 监控设计 | — | [38-monitoring-design.md](./03-basic-design/38-monitoring-design.md) |
| 39 | 03-basic-design | 备份设计 | — | [39-backup-design.md](./03-basic-design/39-backup-design.md) |
| 40 | 03-basic-design | 迁移设计 | — | [40-migration-design.md](./03-basic-design/40-migration-design.md) |
| 41 | 03-basic-design | 基本设计评审记录 | BD Review | [41-bd-review-record.md](./03-basic-design/41-bd-review-record.md) |
| 42 | 04-detailed-design | 程序结构图 | — | [42-program-structure.md](./04-detailed-design/42-program-structure.md) |
| 43 | 04-detailed-design | 模块设计书 | — | [43-module-design.md](./04-detailed-design/43-module-design.md) |
| 44 | 04-detailed-design | 类图 | — | [44-class-diagram.md](./04-detailed-design/44-class-diagram.md) |
| 45 | 04-detailed-design | 逻辑规格 | — | [45-logic-specification.md](./04-detailed-design/45-logic-specification.md) |
| 46 | 04-detailed-design | API 详细规格 | — | [46-api-detailed-spec.md](./04-detailed-design/46-api-detailed-spec.md) |
| 47 | 04-detailed-design | 数据库详细设计 | — | [47-db-detailed-design.md](./04-detailed-design/47-db-detailed-design.md) |
| 48 | 04-detailed-design | SQL 设计与评审记录 | — | [48-sql-design-and-review.md](./04-detailed-design/48-sql-design-and-review.md) |
| 49 | 04-detailed-design | 批处理详细规格 | — | [49-batch-detailed-spec.md](./04-detailed-design/49-batch-detailed-spec.md) |
| 50 | 04-detailed-design | 错误处理方针 | — | [50-error-handling-policy.md](./04-detailed-design/50-error-handling-policy.md) |
| 51 | 04-detailed-design | 日志设计 | — | [51-log-design.md](./04-detailed-design/51-log-design.md) |
| 52 | 04-detailed-design | 详细设计评审记录 | DD Review | [52-dd-review-record.md](./04-detailed-design/52-dd-review-record.md) |
| 53 | 05-implementation | 开发环境搭建 | — | [53-dev-env-setup.md](./05-implementation/53-dev-env-setup.md) |
| 54 | 05-implementation | 编码规范 + 实现指南 | PG | [54-coding-conventions.md](./05-implementation/54-coding-conventions.md) |
| 55 | 05-implementation | 静态分析报告 | SAST | [55-sast-report.md](./05-implementation/55-sast-report.md) |
| 56 | 05-implementation | 代码评审记录 | CR | [56-code-review-record.md](./05-implementation/56-code-review-record.md) |
| 57 | 05-implementation | 构建产物管理台账 | Build | [57-build-artifact.md](./05-implementation/57-build-artifact.md) |
| 58 | 05-implementation | CI 流水线配置 | CI | [58-ci-pipeline.md](./05-implementation/58-ci-pipeline.md) |
| 59 | 06-unit-test | 单元测试计划 | UT Plan | [59-ut-plan.md](./06-unit-test/59-ut-plan.md) |
| 60 | 06-unit-test | 单元测试规格 | UT | [60-ut-specification.md](./06-unit-test/60-ut-specification.md) |
| 61 | 06-unit-test | 单元测试评审记录 | UT Review | [61-ut-review-record.md](./06-unit-test/61-ut-review-record.md) |
| 62 | 06-unit-test | 单元测试结果 | UT | [62-ut-execution-report.md](./06-unit-test/62-ut-execution-report.md) |
| 63 | 06-unit-test | Bug 票 | Bug Fix | [63-bug-ticket.md](./06-unit-test/63-bug-ticket.md) |
| 64 | 06-unit-test | 回归 / 再测记录 | Retest | [64-retest-record.md](./06-unit-test/64-retest-record.md) |
| 65 | 06-unit-test | 单元测试完成批准 | — | [65-ut-completion-approval.md](./06-unit-test/65-ut-completion-approval.md) |
| 66 | 07-integration-test | 集成测试计划 | IT Plan | [66-it-plan.md](./07-integration-test/66-it-plan.md) |
| 67 | 07-integration-test | 集成测试规格 | IT | [67-it-specification.md](./07-integration-test/67-it-specification.md) |
| 68 | 07-integration-test | 集成测试环境搭建 | — | [68-it-env-setup.md](./07-integration-test/68-it-env-setup.md) |
| 69 | 07-integration-test | 内部集成测试结果 | ITa | [69-ita-report.md](./07-integration-test/69-ita-report.md) |
| 70 | 07-integration-test | 外部集成测试结果 | ITb | [70-itb-report.md](./07-integration-test/70-itb-report.md) |
| 71 | 07-integration-test | API 集成测试结果 | — | [71-api-it-report.md](./07-integration-test/71-api-it-report.md) |
| 72 | 07-integration-test | 数据库集成测试结果 | — | [72-db-it-report.md](./07-integration-test/72-db-it-report.md) |
| 73 | 07-integration-test | 外部系统联动测试结果 | — | [73-external-if-it-report.md](./07-integration-test/73-external-if-it-report.md) |
| 74 | 07-integration-test | 问题处理记录 | — | [74-issue-handling-record.md](./07-integration-test/74-issue-handling-record.md) |
| 75 | 07-integration-test | 回归测试结果 | Regression | [75-regression-report.md](./07-integration-test/75-regression-report.md) |
| 76 | 08-system-test | 系统测试计划 | ST Plan | [76-st-plan.md](./08-system-test/76-st-plan.md) |
| 77 | 08-system-test | 系统测试规格 | ST | [77-st-specification.md](./08-system-test/77-st-specification.md) |
| 78 | 08-system-test | 功能(端到端)系统测试结果 | — | [78-functional-st-report.md](./08-system-test/78-functional-st-report.md) |
| 79 | 08-system-test | 业务场景测试结果 | — | [79-scenario-st-report.md](./08-system-test/79-scenario-st-report.md) |
| 80 | 08-system-test | 性能测试结果 | PT | [80-pt-report.md](./08-system-test/80-pt-report.md) |
| 81 | 08-system-test | 负载测试结果 | Load Test | [81-load-test-report.md](./08-system-test/81-load-test-report.md) |
| 82 | 08-system-test | 压力测试结果 | Stress | [82-stress-test-report.md](./08-system-test/82-stress-test-report.md) |
| 83 | 08-system-test | 安全测试结果 | Security | [83-security-st-report.md](./08-system-test/83-security-st-report.md) |
| 84 | 08-system-test | 故障注入测试 | — | [84-failure-injection-report.md](./08-system-test/84-failure-injection-report.md) |
| 85 | 08-system-test | 恢复测试 | Recovery | [85-recovery-test-report.md](./08-system-test/85-recovery-test-report.md) |
| 86 | 08-system-test | 备份恢复测试 | B/R | [86-backup-restore-report.md](./08-system-test/86-backup-restore-report.md) |
| 87 | 08-system-test | 可用性测试 | — | [87-availability-test-report.md](./08-system-test/87-availability-test-report.md) |
| 88 | 08-system-test | 运维测试 | OT | [88-ot-report.md](./08-system-test/88-ot-report.md) |
| 89 | 08-system-test | 系统测试完成批准 | — | [89-st-completion-approval.md](./08-system-test/89-st-completion-approval.md) |
| 90 | 09-uat | 验收测试计划 | UAT Plan | [90-uat-plan.md](./09-uat/90-uat-plan.md) |
| 91 | 09-uat | 验收测试规格 | UAT | [91-uat-specification.md](./09-uat/91-uat-specification.md) |
| 92 | 09-uat | 验收测试结果 | UAT | [92-uat-result-report.md](./09-uat/92-uat-result-report.md) |
| 93 | 09-uat | 业务场景综合结果 | — | [93-business-scenario-report.md](./09-uat/93-business-scenario-report.md) |
| 94 | 09-uat | 验收判定 | — | [94-acceptance-decision.md](./09-uat/94-acceptance-decision.md) |
| 95 | 09-uat | 验收书 | Acceptance | [95-acceptance-letter.md](./09-uat/95-acceptance-letter.md) |
| 96 | 10-migration | 迁移计划 | Migration Plan | [96-migration-plan.md](./10-migration/96-migration-plan.md) |
| 97 | 10-migration | 迁移步骤 | — | [97-migration-procedure.md](./10-migration/97-migration-procedure.md) |
| 98 | 10-migration | 迁移演练结果 | Rehearsal | [98-migration-rehearsal-report.md](./10-migration/98-migration-rehearsal-report.md) |
| 99 | 10-migration | 数据迁移结果 | Data Migration | [99-data-migration-report.md](./10-migration/99-data-migration-report.md) |
| 100 | 10-migration | 系统切替记录 | — | [100-system-cutover-record.md](./10-migration/100-system-cutover-record.md) |
| 101 | 10-migration | 迁移结果报告 | — | [101-migration-result-report.md](./10-migration/101-migration-result-report.md) |
| 102 | 11-release | 发布计划 | Release Plan | [102-release-plan.md](./11-release/102-release-plan.md) |
| 103 | 11-release | 发布决策 | Go/No-Go | [103-go-no-go-decision.md](./11-release/103-go-no-go-decision.md) |
| 104 | 11-release | 生产环境搭建 | Production | [104-prod-env-build-record.md](./11-release/104-prod-env-build-record.md) |
| 105 | 11-release | 生产部署 | Deploy | [105-prod-deploy-record.md](./11-release/105-prod-deploy-record.md) |
| 106 | 11-release | 冒烟测试 | Smoke Test | [106-smoke-test-report.md](./11-release/106-smoke-test-report.md) |
| 107 | 11-release | 上线宣言 | Go-Live | [107-go-live-declaration.md](./11-release/107-go-live-declaration.md) |
| 108 | 11-release | 初期密集护航 | Hypercare | [108-hypercare-report.md](./11-release/108-hypercare-report.md) |
| 109 | 12-operations | 运维交接清单 | Handover | [109-handover-checklist.md](./12-operations/109-handover-checklist.md) |
| 110 | 12-operations | 监控仪表盘 | Monitoring | [110-monitoring-dashboard.md](./12-operations/110-monitoring-dashboard.md) |
| 111 | 12-operations | Job 运维 Runbook | Job | [111-job-runbook.md](./12-operations/111-job-runbook.md) |
| 112 | 12-operations | 备份历史 | Backup | [112-backup-history.md](./12-operations/112-backup-history.md) |
| 113 | 12-operations | 容量报告 | Capacity | [113-capacity-report.md](./12-operations/113-capacity-report.md) |
| 114 | 12-operations | 事件报告 | Incident | [114-incident-report.md](./12-operations/114-incident-report.md) |
| 115 | 12-operations | 故障记录 | — | [115-failure-record.md](./12-operations/115-failure-record.md) |
| 116 | 12-operations | 根因分析与永久对策 | Problem | [116-rca-and-permanent-fix.md](./12-operations/116-rca-and-permanent-fix.md) |
| 117 | 12-operations | 咨询 / 工单记录 | Support | [117-support-ticket.md](./12-operations/117-support-ticket.md) |
| 118 | 13-maintenance | 变更要求 | CR | [118-change-request.md](./13-maintenance/118-change-request.md) |
| 119 | 13-maintenance | 影响分析 | Impact Analysis | [119-impact-analysis.md](./13-maintenance/119-impact-analysis.md) |
| 120 | 13-maintenance | 变更批准 | Change | [120-change-approval.md](./13-maintenance/120-change-approval.md) |
| 121 | 13-maintenance | 配置管理台账 | CM | [121-cm-registry.md](./13-maintenance/121-cm-registry.md) |
| 122 | 13-maintenance | 补丁应用 | Patch | [122-patch-record.md](./13-maintenance/122-patch-record.md) |
| 123 | 13-maintenance | 漏洞处理 | Vulnerability | [123-vulnerability-handling.md](./13-maintenance/123-vulnerability-handling.md) |
| 124 | 13-maintenance | 维护性发布 | Maintenance | [124-maintenance-release-notes.md](./13-maintenance/124-maintenance-release-notes.md) |
| 125 | 13-maintenance | 紧急修复 | Hotfix | [125-hotfix-report.md](./13-maintenance/125-hotfix-report.md) |
| 126 | 13-maintenance | 维护回归测试 | Regression | [126-maintenance-regression-report.md](./13-maintenance/126-maintenance-regression-report.md) |
| 127 | 14-quality | 质量计划 | QA Plan | [127-qa-plan.md](./14-quality/127-qa-plan.md) |
| 128 | 14-quality | 质量评审 | QA Review | [128-qa-review-record.md](./14-quality/128-qa-review-record.md) |
| 129 | 14-quality | 质量评估 | QA | [129-qa-assessment-report.md](./14-quality/129-qa-assessment-report.md) |
| 130 | 14-quality | 质量审计 | Audit | [130-quality-audit.md](./14-quality/130-quality-audit.md) |
| 131 | 15-management | 项目计划 | PJ Plan | [131-project-plan.md](./15-management/131-project-plan.md) |
| 132 | 15-management | 工作分解结构 | WBS | [132-wbs.md](./15-management/132-wbs.md) |
| 133 | 15-management | 进度报告 | Progress | [133-progress-report.md](./15-management/133-progress-report.md) |
| 134 | 15-management | 问题台账 | Issue | [134-issue-list.md](./15-management/134-issue-list.md) |
| 135 | 15-management | 风险台账 | Risk | [135-risk-register.md](./15-management/135-risk-register.md) |
| 136 | 15-management | 范围变更记录 | Change | [136-scope-change-record.md](./15-management/136-scope-change-record.md) |
| 137 | 15-management | 配置管理(管理层) | CM | [137-cm-registry-management.md](./15-management/137-cm-registry-management.md) |
| 138 | 15-management | 成果物清单 | Deliverable | [138-deliverable-list.md](./15-management/138-deliverable-list.md) |
| 139 | 15-management | 评审管理台账 | Review | [139-review-management.md](./15-management/139-review-management.md) |
| 140 | 15-management | 会议 / 报告模板 | Meeting/Report | [140-meeting-and-report.md](./15-management/140-meeting-and-report.md) |
| 141 | 15-management | 工时报告 | Effort | [141-effort-report.md](./15-management/141-effort-report.md) |
| 142 | 15-management | 成本报告 | Cost | [142-cost-report.md](./15-management/142-cost-report.md) |
| 143 | 15-management | 范围变更(管理层) | Scope | [143-scope-change-record.md](./15-management/143-scope-change-record.md) |
| 144 | 15-management | 基线管理 | Baseline | [144-baseline-registry.md](./15-management/144-baseline-registry.md) |
| 145 | 16-closure | 项目完成判定 | — | [145-project-completion-decision.md](./16-closure/145-project-completion-decision.md) |
| 146 | 16-closure | 成果物移交 | Handover | [146-deliverable-handover.md](./16-closure/146-deliverable-handover.md) |
| 147 | 16-closure | 完结报告 | Closure | [147-closure-report.md](./16-closure/147-closure-report.md) |
| 148 | 16-closure | 回顾 + 改进 | Retrospective | [148-retrospective.md](./16-closure/148-retrospective.md) |
| 149 | 16-closure | 知识移交 | KT | [149-knowledge-transfer.md](./16-closure/149-knowledge-transfer.md) |
| 150 | 16-closure | 归档完成 | Archive | [150-archive-completion.md](./16-closure/150-archive-completion.md) |

## 维护

- 模板生成脚本:`scripts/gen_workflow_templates.py`
- 重新生成:删除本目录后,运行 `python scripts\gen_workflow_templates.py`
- 模板版本随项目演进;变更请更新生成脚本。
