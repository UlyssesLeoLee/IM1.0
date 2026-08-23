---
doc_id: aux-07
title_ja: SQL 最適化チェックリスト
title_zh: SQL 优化 Checklist
phase: 04-detailed-design-aux
owners: DBA + 開発者
status: Draft
version: 1.0.0
related_activities: 47 DB 详细, 48 SQL 设计, 80 性能试验
---

# aux-07. SQL 最適化チェックリスト / SQL 优化 Checklist

> 项目: **IM1.0**
> 阶段: 詳細設計 / Detailed Design(辅助)
> 责任方: DBA + 開発者

## 1. 目的 (Purpose)

为关键 SQL 提供标准化的检查表,避免常见性能 / 正确性陷阱。

## 2. 适用范围 (Scope)

生产环境运行的关键 SQL(单次执行 > 100ms 或日调用 > 1 万)。

## 3. 责任方 (Owners)

DBA + 開発者

## 4. 前置依赖 (Prerequisites / Inputs)

- 48 SQL 设计
- DB 详细(47)

## 5. 输出 / 模板正文 (Body)

## A. 必查项(任何 SQL 上线前)

- [ ] **避免 SELECT \***:只查需要的列
- [ ] **WHERE 用索引列**:不绕开(`WHERE LOWER(email) = ...` 走不上索引 → 改函数索引或归一化存储)
- [ ] **JOIN 列类型一致**:避免隐式转换(数值 vs 字符串)
- [ ] **LIMIT / OFFSET 合理**:深分页用 `WHERE id > ?` 替代
- [ ] **COUNT 谨慎**:`COUNT(*)` 全表扫描;大表用 `EXISTS` 或近似
- [ ] **子查询 vs JOIN**:相关子查询多次执行,优先 JOIN
- [ ] **ORDER BY 索引列**:避免 filesort
- [ ] **GROUP BY 用索引列**
- [ ] **DISTINCT / UNION 去重**:检查是否有必要
- [ ] **事务短小**:不持锁做计算
- [ ] **预编译**:避免 SQL 注入 + 缓存执行计划

## B. 索引检查

- [ ] 高频 WHERE 条件有索引
- [ ] JOIN 关联列有索引
- [ ] ORDER BY 列有索引
- [ ] 复合索引最左前缀匹配
- [ ] 索引选择性 > 5%(否则不如全表)
- [ ] 不创建冗余索引
- [ ] 不在小表上建索引(< 1k 行)

## C. 反模式(必须避免)

| 反模式 | 原因 | 替代 |
|---|---|---|
| `SELECT *` | 带宽 + 隐式 IO | 显式列 |
| `WHERE function(col) = ?` | 索引失效 | 函数索引或归一化 |
| `LIKE '%xx%'` 前导通配 | 索引失效 | 全文索引 / ngram |
| `OR col = ? OR col = ?` | 可能不走索引 | `IN (?, ?)` |
| `NOT IN (SELECT ...)` | 难优化 | `NOT EXISTS` |
| 大事务 | 锁竞争 | 分批 |
| 隐式类型转换 | 索引失效 | 类型一致 |
| `OFFSET 100000` 深分页 | 慢 | 游标 / 范围分页 |

## D. 性能阈值

| SQL 类型 | 期望 |
|---|---|
| 主键点查 | < 1ms |
| 索引范围 | < 10ms |
| 简单聚合(全表) | < 100ms |
| 复杂分析 | < 1s |
| 报表 | < 5s(可异步) |

## E. 执行计划解读

```
Seq Scan on users  (cost=0.00..1234.00 rows=10000)
  Filter: (status = 1)
```

- ❌ Seq Scan:全表扫描,大表不可接受
- ✅ Index Scan:走索引
- ✅ Index Only Scan:覆盖索引,最优

## F. 慢查询处理流程

1. **发现**:监控 / 用户反馈 / 慢日志
2. **定位**:`EXPLAIN ANALYZE` 看执行计划
3. **优化**:
   - 加索引
   - 改写 SQL
   - 拆分
   - 缓存
4. **验证**:压测确认
5. **回归**:加入性能基线

## G. 评审 Checklist

- [ ] EXPLAIN 已附在 PR
- [ ] 索引已加(若有)
- [ ] 性能已测(基准 + 峰值)
- [ ] 死锁 / 锁等待已分析
- [ ] 资源消耗(CPU / IO)可接受


## 6. 验收标准 (Acceptance Criteria)

每条关键 SQL 上线前有 EXPLAIN + 评审;性能回归 CI 阻断。

## 7. 关联文档 (References)

- 关联工程活动: 47 DB 详细, 48 SQL 设计, 80 性能试验
- 上游 Workflow: `docs/Workflow.md` Phase 4

## 8. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (待定) | 初版 |
