# 保守 / Maintenance
> 工程范围: No.118-126
> 阶段目的: CR→Impact→Approval→CM→Patch→Vuln→Maintenance→Hotfix→Regression
> 出口 GATE: **变更关闭**

## 文档清单
| 工程 | 文档 | 略称 | 责任方 | 文件 |
|---|---|---|---|---|
| 118 | 変更要求票 (CR) / 变更要求 | CR | PM / 业务側 | [118-change-request.md](118-change-request.md) |
| 119 | 影響分析書 / 影响分析 | Impact Analysis | Tech Lead + 開発者 | [119-impact-analysis.md](119-impact-analysis.md) |
| 120 | 変更承認記録 / 变更批准 | Change | CAB | [120-change-approval.md](120-change-approval.md) |
| 121 | 構成管理台帳 / 配置管理台账 | CM | DevOps + Tech Lead | [121-cm-registry.md](121-cm-registry.md) |
| 122 | パッチ適用記録 / 补丁应用 | Patch | SRE | [122-patch-record.md](122-patch-record.md) |
| 123 | 脆弱性対応記録 / 漏洞处理 | Vulnerability | セキュリティ + SRE | [123-vulnerability-handling.md](123-vulnerability-handling.md) |
| 124 | 改修リリースノート / 维护性发布 | Maintenance | PM + Tech Lead | [124-maintenance-release-notes.md](124-maintenance-release-notes.md) |
| 125 | Hotfix レポート / 紧急修复 | Hotfix | Tech Lead + SRE | [125-hotfix-report.md](125-hotfix-report.md) |
| 126 | 保守回帰試験結果 / 维护回归测试 | Regression | QA | [126-maintenance-regression-report.md](126-maintenance-regression-report.md) |

## 使用建议
1. 进入本 Phase 前,确认上游 GATE(变更关闭 的上游)已通过。
2. 按 No. 顺序逐项填写对应文档;不必强求一次写满,可迭代。
3. 完成本 Phase 全部文档后,申请 变更关闭 评审;通过后才能进入下一 Phase。
4. 残留问题 / 范围变更走 `docs/templates/15-management/136-scope-change-record.md`。
