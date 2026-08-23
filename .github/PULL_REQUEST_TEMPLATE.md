# Pull Request

## 描述

<!-- 简述本次 PR 改了什么 / 为什么 -->

## 类型

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to change)
- [ ] Documentation
- [ ] Refactor (no functional change)
- [ ] Performance
- [ ] Test

## 关联

- [ ] 关联 Issue: #
- [ ] 关联 SRS 需求 ID: <!-- 例: IM-FR-003 -->
- [ ] 关联 ADR: <!-- 例: ADR-014 -->
- [ ] 关联 Workflow 活动: <!-- 例: 42 程序结构 / 46 API 详细 -->

## 协议冻结 (Protocol Freeze)

- [ ] **未变更** wire format (WS 帧 / gRPC 字段 / REST 路径)
- [ ] **已变更** wire format —— 已同步更新:
  - [ ] `docs/DetailedDesign.md` 对应章节
  - [ ] `docs/templates/04-detailed-design/aux/aux-13-protocol-frame-samples.md`
  - [ ] `docs/ImplementationSpec.md §3`
  - [ ] `crates/im-proto/proto/*.proto` (重新生成)

## Checklist (aux-10 DD Review 简化版)

### 命名 (aux-01)
- [ ] 无 Core Schema 游戏专有字段(guild_id / match_id / party_id 等)
- [ ] 用 `conversation` 而非 `room` / `chat`
- [ ] 用 `delivery_state` 而非 `status`(在 messages 表)

### 错误码 (aux-03)
- [ ] 所有 `ErrorCode::Xxx` 已在 aux-03 §B 注册
- [ ] HTTP 状态码与 §B 一致
- [ ] IDEMPOTENCY_CONFLICT 走成功语义(WS `ok=true` + REST HTTP 200)

### 数据字典 (aux-02)
- [ ] 所有新表字段在 aux-02 §F 有对应子表
- [ ] 隐私级别标注正确(公开/内部/敏感/机密)
- [ ] 索引策略在 §4.4 索引检查表内

### 协议帧 (aux-13)
- [ ] WS 帧样例与 `crates/im-protocol/src/ws_frames.rs` 一致
- [ ] REST 错误响应含 `code` / `message` / `trace_id` / `ts`

### Rust 代码 (ImplementationSpec §7)
- [ ] trait 接口签名与 ImplementationSpec §7.4-§7.6 一致
- [ ] `AppError` ⇄ `ErrorCode` 单一来源未破坏
- [ ] 模块间不反向依赖(无 `im-core` → `im-gateway` 等)

## 测试

- [ ] 单元测试已加(行覆盖 ≥ 60%,关键模块 ≥ 80%)
- [ ] Integration Test 已加(若涉及 DB / API)
- [ ] 本地 `cargo test --workspace` 全绿
- [ ] SAST 通过(cargo audit + clippy + semgrep)

## 部署

- [ ] 改动不需新增 K8s Secret
- [ ] 改动不需修改 K3s manifests
- [ ] 改动不需修改 Dockerfile
- [ ] (如需要) 已同步更新 deploy/k3s/dev/

## 备注

<!-- 任何 reviewer 需知的事项 -->
