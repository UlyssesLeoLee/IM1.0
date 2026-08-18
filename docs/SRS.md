# 可嵌入式游戏 IM 平台 软件需求规格说明书（SRS）

版本：v0.1 Draft
状态：待评审（Architecture Review 已完成第一轮内部修订，见第 39 章）

---

## 1. Executive Summary

本产品是一套 **IM-First** 的实时通信平台：核心是一个可独立部署、可靠、低延迟的即时通信系统（IM Core），游戏是其首要垂直应用场景而非核心组成部分。产品通过 Extension Runtime 向游戏、AI、工作场景扩展能力，扩展的成功或失败不得影响 IM Core 的可用性。客户端侧以 SDK 为第一产品，官方 UI（含 Laser HUD）是可选参考实现之一。

优先级铁律：**IM 可靠性 > 消息正确性 > 安全 > 游戏集成体验 > 低延迟 > 可扩展性 > HUD 体验 > AI 能力 > 技术先进性**。

## 2. Product Vision

成为游戏、桌面软件与 AI/工作场景的"默认可嵌入 IM 层"——像 Stripe 之于支付一样，让任何游戏或应用以极低成本获得可靠的实时通信能力，同时保留向 AI 与协作场景自然扩展的空间。

## 3. Product Positioning

- 不是 Discord/Slack 克隆，不做社区门户与内容分发。
- 不是游戏服务器框架，不管理房间/匹配的权威游戏状态。
- 不是 AI Agent 平台，AI 是消息通道之上的一个订阅者。
- 是：**嵌入式 IM 基础设施 + 游戏身份桥接 + 低干扰 HUD 客户端 + 可插拔扩展运行时**。

## 4. Goals

- G1 提供稳定、有序、可去重、可离线同步的消息能力（IM Core）。
- G2 游戏可在数小时到数天内完成 IM 接入（SDK-first）。
- G3 提供不打断游戏/工作流的低干扰访问方式（Laser HUD）。
- G4 允许 AI、Work 等能力以扩展形式接入，故障隔离。
- G5 支持从个人开发者单机部署到百万级并发的弹性伸缩路径。
- G6 提供多租户 SaaS 与私有化部署两种交付形态。

## 5. Non-Goals（第一阶段）

- 完整 Discord/Slack 替代品；社区/服务器发现门户。
- Jira/项目管理替代品；通用工作流引擎。
- 游戏引擎、游戏服务器权威状态框架（Match/战斗结算等）。
- 通用 AI Agent Framework（编排/工具链管理）。
- 社交媒体 Feed、直播、视频会议。

任何此类能力若被提出，必须证明其直接服务于"可靠低延迟 IM + 低成本嵌入 + 低干扰访问"这一核心价值，否则拒绝纳入路线图。

## 6. Personas

| Persona | 描述 | 核心诉求 |
|---|---|---|
| 游戏客户端开发者 | Unity/Unreal/自研引擎工程师 | 几行代码接入聊天，不理解后端 |
| 游戏后端/运维 | 负责服务器权威逻辑 | 防作弊、Guild/Party 状态同步、可观测 |
| 玩家 | 游戏内用户 | 不重复注册、消息不丢不重复、HUD 不挡视野 |
| IM 平台运维（SRE） | 私有部署或 SaaS 运营方 | K3s 上稳定运行、故障可定位 |
| AI/工作场景集成方 | 接入 Bot、CI 通知 | 通过标准 Extension API 接入，不改核心协议 |
| 产品/合规负责人 | 关注隐私、审计、多租户隔离 | 数据可删除、可导出、租户隔离 |

## 7. Use Cases（节选，完整用例见附录）

- UC-01 玩家使用游戏发行商账号首次登录，自动创建/关联 IM Identity。
- UC-02 玩家加入 Guild，游戏扩展自动创建/加入对应 Conversation。
- UC-03 玩家在全屏无边框游戏中按住快捷键呼出 Laser→Peek→Chat。
- UC-04 玩家断线重连，收到期间的离线消息且不重复、不乱序。
- UC-05 AI Observer 对一个允许访问的频道生成会议摘要，不打断用户。
- UC-06 CI 构建失败，Work Extension 向指定 Conversation 推送系统消息。
- UC-07 运维在 K3s 上仅部署 IM Core（无 Extension Runtime）验证基础聊天。
- UC-08 租户 A 的 Guild 消息与租户 B 完全隔离，密钥/配额独立计量。

## 8. System Context

```
[Game Client / Desktop App / Web] --SDK--> [im-gateway] --> [im-core] --event--> [Extension Runtime: Game/AI/Work]
                                                 |                              
                                          [Presence/Media/Notification]
                                                 |
                                    [PostgreSQL / Valkey / NATS JetStream / MinIO]
```

外部系统：Steam/Xbox/PSN/Epic/自定义 OIDC（身份）、GitHub/GitLab/Jira（Work）、LLM Provider（AI，可选）。

## 9. Domain Model（核心，稳定，不被 Extension 污染）

实体：User, Guest, DeviceSession, Token, Relationship(Friend/Block/Follow), Conversation(type: dm/group/channel/system/broadcast), Message, Reaction, Presence, Notification, MediaObject。

扩展点：`Conversation.metadata`（namespaced key，如 `game.*`, `ai.*`, `work.*`）、`Message.custom_payload`（结构化消息）、Event Bus 上的领域事件（`message.created` 等）。**Core 表结构中不得出现 `guild_id`、`match_id` 等游戏专有字段** —— 这些只能存在于 metadata 或 Extension 自有存储中。

## 10. IM Core Requirements

| ID | Title | Priority | Acceptance Criteria |
|---|---|---|---|
| IM-FR-001 | Core 可独立部署运行，无 Game/AI/Work 依赖 | P0 | 关闭 Extension Runtime 后基础消息收发、Presence、好友功能仍可用 |
| IM-FR-002 | 消息具备全局唯一 Message ID 与 Conversation 内单调 Sequence | P0 | 并发发送 1000 条消息，Sequence 无重复、无回退 |
| IM-FR-003 | 客户端发送具备幂等性（Client Token 去重） | P0 | 同一 idempotency key 重复提交只产生一条消息 |
| IM-FR-004 | 消息投递状态机：Sent→Delivered→Read，支持失败重试 | P0 | 状态转换可查询，重试上限与退避策略可配置 |
| IM-FR-005 | 离线消息与增量同步（Cursor-based） | P0 | 客户端断线 N 分钟后重连，按 cursor 拉取增量，无丢失 |

（完整 IM-FR 编号延续至 040+，覆盖 reply/quote/mention/reaction/edit/delete/recall/forward/pin，此处收录关键项，其余见 Traceability Matrix 附录。）

## 11. Identity Requirements

| ID | Title | Priority |
|---|---|---|
| IM-ID-001 | 支持 User / Guest / External Identity 三类主体 | P0 |
| IM-ID-002 | 多设备 Device Session，支持同时在线与单点登出 | P0 |
| IM-ID-003 | Token 采用短期 Access Token + Refresh Token，支持 Rotation | P0 |
| IM-ID-004 | Account State 机制：Active/Banned/Deleted/Suspended，状态变更需审计 | P0 |
| IM-ID-005 | Account Linking：Guest → 正式账号升级，保留历史消息 | P1 |

## 12. Relationship Requirements

| ID | Title | Priority |
|---|---|---|
| IM-REL-001 | 好友/申请/黑名单/Follow/Recent Contacts 基础模型 | P0 |
| IM-REL-002 | **游戏扩展可声明"关闭传统好友系统"**，改用 Guild/Party 成员关系驱动 Conversation 可见性 | P1 |

分析：大逃杀类游戏往往没有强好友需求，仅需临时 Party 聊天；MMO 则需要持久好友关系。Core 保留好友模型但设为**可选启用的 Capability**，由 Tenant/Game 配置决定是否暴露好友 UI 与 API，避免为不需要的游戏强制维护社交图谱。

## 13. Conversation Requirements

| ID | Title | Priority |
|---|---|---|
| IM-CONV-001 | 支持 DM / Group / Channel / System / Broadcast 五类会话 | P0 |
| IM-CONV-002 | Conversation 支持任意 namespaced metadata，不影响 Core Schema | P0 |
| IM-CONV-003 | Broadcast 会话为只读单向下行，不产生成员已读状态膨胀 | P1 |

## 14. Message Requirements

| ID | Title | Priority |
|---|---|---|
| IM-MSG-001 | 消息类型：Text/Image/File/Voice/Video/Sticker/System/Custom/Structured | P0 |
| IM-MSG-002 | Reply/Quote/Mention/Reaction/Edit/Delete/Recall/Forward/Pin 全部支持 | P0/P1 |
| IM-MSG-003 | Recall 有时间窗口限制（可配置），超窗后仅可 Delete（本地隐藏） | P1 |
| IM-MSG-004 | Retention 策略按租户/会话类型可配置（如 Guild 频道 90 天，DM 永久） | P1 |

## 14a. Technology Stack Constraints（用户明确约束，覆盖全文档技术选型）

- **后端语言**：Rust 为主（im-gateway/im-core/im-presence/im-media/extension-runtime 等所有性能敏感与长期在线服务）。**Python 为辅**，仅用于 AI Extension 的模型编排/数据处理类组件，以及内部工具脚本、离线批处理；Python 组件必须通过 Extension Runtime 的标准 API/事件边界接入，不得进入消息主路径（呼应 EXT-FR-003 / PERF 红线）。
- **前端**：官方 Full Client（管理后台 Dashboard、Web 版 Full Client、SaaS 控制台）统一使用 **Next.js**。HUD 的 Desktop Overlay 渲染层不强制 Next.js（需原生低延迟渲染，可用 Tauri/原生窗口+WebView 或轻量渲染引擎，Needs PoC），但配置类界面（如 HUD 设置面板）可复用 Next.js 组件通过 WebView 承载。
- **开源与许可证约束（TS-NFR-001，P0 红线）**：全部依赖必须是开源且许可证允许商业使用（MIT/Apache-2.0/BSD/MPL 等 Permissive 或兼容 Copyleft），**禁止引入 AGPL、SSPL、BUSL 等限制商业闭源使用或要求开放服务端源码的许可证**，禁止任何专有/商业授权组件（除非租户自行选择接入外部专有服务，如商业 LLM API，且明确为可选 Extension 依赖而非平台强制依赖）。技术选型评审（ADR）必须包含许可证检查项。
- 该约束回填至：Rust 技术选型（第15/后端）、Python 用于 AI Extension（第27章）、Next.js 用于 Dashboard/Full Client（第22/23章）、以及所有基础设施组件（PostgreSQL/Valkey/NATS JetStream/MinIO均为开源许可证，符合约束，见第40章）。

## 15. Presence Requirements

| ID | Title | Priority |
|---|---|---|
| IM-PRES-001 | Online/Offline/Away/Busy/Invisible/Custom 状态机 | P0 |
| IM-PRES-002 | Typing、Last Seen、Device Presence | P1 |
| IM-PRES-003 | Game Presence 与 Custom Presence 作为 Extension 写入的 metadata，Core 只透传 | P1 |
| IM-PRES-004 | 大规模 Presence 广播需限频/聚合，避免 Guild 内每次上下线全员广播 | P0（性能约束） |

## 16. Notification Requirements

| ID | Title | Priority |
|---|---|---|
| IM-NOTIF-001 | Desktop/Mobile Push/In-game/HUD 四通道 | P0/P1 |
| IM-NOTIF-002 | Priority、Mention、Mute、DND 规则 | P0 |
| IM-NOTIF-003 | AI/Work 来源通知默认走"低优先级/不打断"通道 | P1 |

## 17. Media Requirements

| ID | Title | Priority |
|---|---|---|
| IM-MEDIA-001 | 媒体对象存储于 MinIO（S3 兼容），消息仅存引用 | P0 |
| IM-MEDIA-002 | 上传采用预签名 URL，避免媒体流经 im-core 路径 | P0 |
| IM-MEDIA-003 | 病毒/内容扫描为可选 Extension（Moderation） | P2 |

## 18. Search Requirements

| ID | Title | Priority |
|---|---|---|
| IM-SEARCH-001 | 会话内关键字搜索（PostgreSQL 全文索引） | P1 |
| IM-SEARCH-002 | 全局跨会话搜索为 V2，需权限过滤 | P2 |

## 19. Game Integration Requirements

| ID | Title | Priority |
|---|---|---|
| GAME-FR-001 | 提供统一 Client SDK：initialize→authenticate→connect→join→send 五步 | P0 |
| GAME-FR-002 | 官方 SDK 覆盖 Unity、Unreal、C ABI（供 C/C++/其他引擎绑定） | P0(Unity/Unreal) / P1(其他) |
| GAME-FR-003 | Rust 编写核心 SDK 逻辑，通过 C ABI 暴露，各引擎绑定为薄封装层 | Candidate（需 PoC-03/04 验证跨平台可行性） |
| GAME-FR-004 | 开发者无需理解内部微服务拓扑，SDK 内部处理重连/心跳/序列 | P0 |

**ADR 提示**：Godot/JS/Web SDK 可直接走 WebSocket/gRPC-Web，无需 C ABI；C ABI 主要服务 Unity(IL2CPP)/Unreal(C++)/原生游戏。见 ADR-008。

## 20. Game Identity Requirements

| ID | Title | Priority |
|---|---|---|
| GAME-ID-001 | External Identity → Identity Mapping → IM Identity 三层模型 | P0 |
| GAME-ID-002 | 支持 Steam/Xbox/PSN/Nintendo/Epic/Guest/自定义 JWT-OIDC 接入 | P0(Guest+JWT) / P1(平台方) |
| GAME-ID-003 | **Server Authentication 强制**：游戏客户端不得直接持有可签发 IM Token 的密钥；Token 必须由游戏服务器用 Server Secret 向 IM 平台换取，再下发给客户端 | P0（安全红线） |
| GAME-ID-004 | Guest Upgrade / Account Merge 需要冲突检测与人工/规则化裁决流程 | P1 |
| GAME-ID-005 | Cross-game / Cross-platform Identity 为 V2，需租户显式授权 | P2 |

防伪造设计：游戏客户端永远不能自签发身份声明；所有身份主张必须经由游戏服务器（authoritative）通过 Server-to-Server API 完成 Token Exchange，IM Gateway 校验 Token 签名与租户绑定。

## 21. Game Social Mapping

| ID | Title | Priority |
|---|---|---|
| GAME-SOC-001 | Guild/Clan/Party/Team/Match/Lobby/World/Alliance/Region/Server/Spectator 均由 Game Extension 映射为 Conversation + metadata，Core 不新增字段 | P0（架构约束） |
| GAME-SOC-002 | Match/Lobby 类临时会话需自动 TTL 回收；Guild 解散需级联归档而非物理删除（审计需要） | P1 |
| GAME-SOC-003 | Party 自动加入/离开由 Extension 监听游戏服务器事件驱动，IM Core 只响应标准"加入/离开会话"API | P0 |
| GAME-SOC-004 | 大型 Guild（>5000 成员）与 World Channel 采用只读广播 + 摘要拉取模式，避免全量 Fan-out | P1（性能约束，见 PERF 章节） |
| GAME-SOC-005 | 跨服/跨区聊天通过独立的 Broadcast Conversation + 区域网关聚合，不要求全局强一致 Presence | P2 |

## 22. SDK Requirements

| ID | Title | Priority |
|---|---|---|
| SDK-FR-001 | 五步接入模型（init/auth/connect/join/send）为跨引擎统一契约 | P0 |
| SDK-FR-002 | SDK 内置重连、心跳、消息去重、本地队列 | P0 |
| SDK-FR-003 | Headless Client 模式（无 UI，仅协议+状态机）供自定义 UI 使用 | P0 |
| SDK-FR-004 | SDK 遥测（连接成功率、延迟）可选上报，默认可关闭 | P1 |

## 23. HUD Requirements

| ID | Title | Priority |
|---|---|---|
| HUD-FR-001 | 默认态仅显示 2~4px 左侧激光光轨（Level 0 Laser） | P0 |
| HUD-FR-002 | 四层交互：Laser→Peek→Chat→Full Client，逐级展开 | P0 |
| HUD-FR-003 | Laser 状态表达仅用 pulse/glow/wave/intensity，不使用高饱和 RGB 灯效轮播 | P0（产品原则） |
| HUD-FR-004 | Peek 层数秒内完成信息浏览（最近消息/@mention/AI 通知/游戏邀请/系统消息） | P1 |
| HUD-FR-005 | HUD 是 Client Layer，服务器不感知/不依赖 HUD 状态 | P0（架构约束） |

## 24. HUD Interaction Model

| ID | Title | Priority |
|---|---|---|
| HUD-IX-001 | 支持 Hold-to-Chat 与 Toggle 两种触发模式，可配置 | P0 |
| HUD-IX-002 | 键盘/鼠标/手柄/触屏/掌机输入均需适配，手柄需明确唤出键位与遮罩 Chat 层输入焦点方案 | P0(键鼠) / P1(手柄/触屏) |
| HUD-IX-003 | HUD 呼出不得导致游戏 Raw Input/Cursor Capture 状态错乱（呼出时短暂释放光标，收起后恢复） | P0（红线，见 PoC-06） |
| HUD-IX-004 | 需支持中/日/韩 IME 输入且不与游戏快捷键冲突 | P1 |
| HUD-IX-005 | 多显示器与 Alt-Tab 场景下 HUD 状态需保持一致，不产生残留窗口 | P1 |

## 25. Overlay Compatibility

| ID | Title | Priority |
|---|---|---|
| HUD-OV-001 | 技术调查覆盖 Windows(D3D11/D3D12/Vulkan/OpenGL)、macOS(Metal)、Linux 的官方/低侵入集成方式 | P0（调查项，非实现项） |
| HUD-OV-002 | 默认禁止 DLL Injection / API Hook 类实现；优先方案排序：①引擎原生 Overlay/Widget 插件 ②平台官方 Overlay API（如 Steam Overlay 协作模式）③独立窗口置顶（非侵入 Desktop HUD） | P0（产品原则） |
| HUD-OV-002a | 若确无官方低侵入方案覆盖某图形 API，产品应优先提供"Engine Plugin"或"独立窗口"退化方案，而非默认启用 Hook | P0 |
| HUD-OV-003 | 建立反作弊兼容风险矩阵（EAC/BattlEye/GameGuard/Vanguard），Engine Plugin 路线需与主流反作弊厂商白名单机制对齐 | P0（见 PoC-07 与 Risk Register） |

风险矩阵（初版，需 PoC-07 验证）：

| 集成模式 | EAC | BattlEye | Vanguard | 侵入性 |
|---|---|---|---|---|
| Engine Plugin（Unity/UE 内置 UI） | 兼容 | 兼容 | 兼容 | 低 |
| 平台官方 Overlay（Steam Overlay 式） | 视具体机制 | 视具体机制 | 视具体机制 | 低 |
| 独立窗口 Desktop HUD（非游戏内渲染） | 兼容 | 兼容 | 兼容 | 最低 |
| DLL Injection/Hook | 高风险，可能被封 | 高风险 | 极高风险（内核级检测） | 高，**默认不采用** |

## 26. Extension Architecture Requirements

| ID | Title | Priority |
|---|---|---|
| EXT-FR-001 | Extension 通过 Manifest 声明 Permission/Scope/Capability/Event Subscription/Command/Webhook | P0 |
| EXT-FR-002 | Extension 运行时隔离（进程/资源配额），单 Extension 崩溃不影响 Core 或其他 Extension | P0（架构红线） |
| EXT-FR-003 | Extension 调用 Core 能力必须走异步事件或独立 API，禁止同步阻塞消息发送主路径 | P0（性能红线，见第27章） |
| EXT-FR-004 | Extension 具备版本化、灰度启停、失败恢复（重试/死信队列） | P1 |
| EXT-FR-005 | 所有 Extension 调用需审计（谁、何时、访问了什么数据） | P0 |

## 27. AI Extension Requirements

| ID | Title | Priority |
|---|---|---|
| AI-FR-001 | AI 以 Bot/Assistant/Moderator/Translator/NPC/Support/Coding/Workflow Agent 等角色接入，均为 Extension | P1 |
| AI-FR-002 | 响应模式：Silent/Mention Only/Observer/Assistant/Autonomous，默认不打断用户 | P1 |
| AI-FR-003 | Observer 模式仅处理显式授权的 Conversation，生成 Summary/Decision/TODO/Risk/Translation/Moderation Result | P1 |
| AI-FR-004 | Context Capsule 动态生成：Recent + Relevant(检索) + Participants + References + Files + Game/Work Context + Memory + Permissions，禁止整段聊天历史直塞 LLM | P1 |
| AI-FR-005 | Context 检索采用 Recent + Vector(可选) + Graph(可选) + Explicit Reference 组合，Vector/Graph 为增强能力非强依赖 | P2 |
| AI-FR-006 | AI 不得跨 Conversation 权限边界读取消息（权限隔离，防 Context Pollution/泄露） | P0（安全红线） |
| AI-FR-007 | 基础 Moderation（Flood/Spam/关键字）不依赖 LLM；AI Moderation 是增强扩展 | P0（架构约束） |

## 28. Work Extension Requirements

| ID | Title | Priority |
|---|---|---|
| WORK-FR-001 | 支持 GitHub/GitLab/Jira/CI-CD/Webhook 事件转 IM 消息（Build Failed/PR Review/Issue Assigned/Deployment/AI Task Complete） | P2 |
| WORK-FR-002 | Work Extension 通过标准 Webhook + Command 接入，不得扩展/改变基础消息协议字段 | P0（架构约束） |

## 29. Multi-Tenant Requirements

| ID | Title | Priority |
|---|---|---|
| TEN-FR-001 | Platform→Tenant→Application/Game→Environment→User 层级模型 | P0 |
| TEN-FR-002 | Tenant/Game/Environment 三级隔离（数据、配置、密钥） | P0 |
| TEN-FR-003 | API Key/Secret 按 Environment 独立签发与轮换 | P0 |
| TEN-FR-004 | Quota/Rate Limit/Billing Meter 按 Tenant 计量 | P1 |
| TEN-FR-005 | 私有化部署下多租户为可选（单租户模式简化部署） | P1 |

## 30. Security Requirements

| ID | Title | Priority |
|---|---|---|
| SEC-NFR-001 | 全链路 TLS，客户端到 Gateway 强制加密 | P0 |
| SEC-NFR-002 | RBAC（管理面）+ ABAC（Conversation/消息级权限，如 Extension 访问范围） | P0 |
| SEC-NFR-003 | Token 短生命周期 + Rotation + 设备绑定；防 Token 提取后长期滥用 | P0 |
| SEC-NFR-004 | Rate Limit / Anti-spam / Anti-bot / Anti-replay（Nonce+时间窗） | P0 |
| SEC-NFR-005 | 游戏客户端视为不可信环境：身份、Guild 成员关系等以**游戏服务器为 Authoritative Source**，IM 仅信任 Server-to-Server 声明 | P0（红线，呼应 GAME-ID-003） |
| SEC-NFR-006 | 审计日志覆盖身份变更、权限变更、Extension 数据访问、管理操作 | P0 |
| SEC-NFR-007 | 静态加密（Encryption at Rest）用于敏感字段与媒体存储 | P1 |

## 31. Moderation Requirements

| ID | Title | Priority |
|---|---|---|
| MOD-FR-001 | Block/Mute/Report/Flood Control 为 Core 基础能力，第一版即具备 | P0 |
| MOD-FR-002 | 关键字策略（Keyword Policy）与 GM/Moderator 工具 | P1 |
| MOD-FR-003 | Evidence Retention（举报证据留存周期可配置） | P1 |
| MOD-FR-004 | Toxicity/AI Moderation 为 Extension，不阻塞消息主路径（先发送后异步复核，或按租户策略选择前置拦截） | P1 |

## 32. Privacy Requirements

| ID | Title | Priority |
|---|---|---|
| PRIV-FR-001 | Delete Account / Delete Message / Data Export 用户自助能力 | P0 |
| PRIV-FR-002 | Data Retention 策略按租户/会话类型配置 | P1 |
| PRIV-FR-003 | AI Data Usage Opt-out、Telemetry Opt-out | P1 |
| PRIV-FR-004 | Region Storage（数据驻留）为 V1/V2 能力，取决于部署形态 | P2 |

**合规声明**：本文档提出满足常见隐私诉求（删除/导出/驻留）的架构能力，但**不声称系统已完全符合 GDPR 或其他特定法域法规**——最终合规需法务评审与租户具体部署配置确认。

## 33. Performance Requirements

见第 34/35 章 SLO 表；核心路径约束：`Client→Gateway→IM Core→Persistence/Event→Fanout→Client`，任何 Extension 不得同步插入此路径（PERF-NFR-001，P0 红线）。

## 34. Availability Requirements

| ID | Title | Priority |
|---|---|---|
| OPS-NFR-001 | IM Core 可用性目标 Candidate：99.9%（月度），需压测验证后转 Confirmed | P0 |
| OPS-NFR-002 | Extension 不可用不得计入 Core 可用性统计（故障域隔离） | P0 |

## 35. Scalability Requirements

并发阶段：10 / 100 / 1K / 10K / 100K / 1M+，需分级验证（见 PoC-01, PoC-10）。Reconnect Storm / Login Storm / Patch-day / Game Launch 流量需专项设计（Gateway 层排队、渐进式 Resume、连接建立限速）。

## 36. Weak Network Requirements

| ID | Title | Priority |
|---|---|---|
| NET-FR-001 | Reconnect/Resume/Retry/Local Queue/去重/增量同步 | P0 |
| NET-FR-002 | 弱网测试场景：高延迟(300ms+)、丢包(5-20%)、间歇性断线 | P0（测试要求） |

## 37. Observability Requirements

OpenTelemetry 贯穿 Client→Gateway→IM Core→Event→Storage→Fanout→Receiver。核心指标：`active_connections`, `connection_latency`, `message_latency`, `message_delivery_latency`, `message_failure_rate`, `reconnect_rate`, `messages_per_second`, `fanout_latency`, `presence_updates`, `queue_depth`, `extension_latency`, `extension_failure_rate`。

## 38. Deployment Requirements / 39. K3s Requirements

支持 Development/Single Node/Small Cluster/HA Cluster/Edge/On-prem/Cloud。初始逻辑边界（**非强制微服务拆分**）：`im-gateway`, `im-core`, `im-presence`, `im-media`, `extension-runtime`。拆分依据仅限：Scaling Boundary（连接数 vs 消息处理）、Failure Boundary（Extension 隔离）、Security Boundary（媒体上传）、Ownership Boundary（团队边界）。禁止"为用 K3s 而拆分"。

后续按负载可拆出：`identity-service`, `relationship-service`, `conversation-service`, `message-service`, `notification-service`——**仅当有实测数据证明单体边界已成为瓶颈**（ADR Required）。

## 40. Data Requirements

IM Core 基线：PostgreSQL（事实存储）+ Valkey（缓存/Presence）+ NATS JetStream（事件/消息总线）+ MinIO（媒体）。**Vector DB（pgvector）与 Graph DB 为 AI Extension 增强能力，IM Core 独立运行不依赖它们**（DATA-NFR-001, P0 架构约束）。

## 41. Disaster Recovery

PostgreSQL 主备+定期备份；NATS JetStream 集群化持久；MinIO 多副本/纠删码；K3s 节点故障需验证 Pod 重调度与会话恢复（PoC-08/09）。RPO/RTO 目标为 Candidate，需压测确定。

## 42. API Requirements / 43. Protocol Requirements

- WebSocket：客户端长连接主通道（消息收发、Presence）。
- HTTP：管理面、历史查询、媒体预签名。
- gRPC：内部服务间、Extension Runtime 与 Core 交互、Server-to-Server（游戏服务器 Token Exchange）。
- QUIC：Candidate，仅用于弱网/移动端优化场景，**不强制所有客户端使用**（PROTO-NFR-001）。

## 44. Compatibility Matrix

| 引擎/语言 | SDK 形态 | 优先级 |
|---|---|---|
| Unity | 官方 SDK（C# + 底层 C ABI 绑定） | P0 |
| Unreal | 官方 SDK（C++ + 底层 C ABI 绑定） | P0 |
| Godot | 官方 SDK（GDExtension 或 WebSocket 直连） | P1 |
| C/C++/Rust | C ABI / 原生 Rust crate | P1 |
| JS/TS/Web | WebSocket/gRPC-Web SDK | P1 |

## 45. MVP Scope

MVP 目标：验证 H1（Rust+K3s IM Core 稳定实时聊天）、H2（低成本嵌入）、H3（Laser HUD 优于传统 Overlay），**不引入 AI/Work Extension**。

MVP 范围（P0 only）：IM-FR-001~005、IM-ID-001~004、IM-CONV-001/002、IM-MSG-001/002（核心子集：text/image/reply/mention/edit/delete/recall）、IM-PRES-001、GAME-FR-001/002(Unity优先)/004、GAME-ID-001~003、HUD-FR-001~005、HUD-IX-001/003、SEC-NFR-001~006、EXT-FR-001/002/003（Extension Runtime 骨架，仅供后续扩展，不实现具体 AI/Work 功能）。

## 46. V1/V2 Roadmap

- **V1**：完善 Guild/Party 社交映射（GAME-SOC-*）、Overlay 兼容矩阵落地、Moderation 完整化、多租户计量、Unreal SDK。
- **V2**：AI Extension（Observer/Assistant）、Work Extension、Cross-game Identity、跨服/跨区聊天、Graph/Vector 增强检索。
- **Future**：Region Storage/多区域部署、Autonomous AI Agent、大型 World Channel 优化。

## 47. PoC Plan

| PoC | Purpose | Environment | Procedure | Metric | Pass | Fail |
|---|---|---|---|---|---|---|
| POC-01 | 验证 100K WebSocket 长连接 | K3s 3节点+压测集群 | 逐步爬升至100K连接，维持30分钟 | 连接成功率、内存/CPU、P99建连延迟 | 成功率≥99%，无OOM | 任一失败 |
| POC-02 | 消息顺序/ACK/重连正确性 | 单集群 | 高并发发送+随机断线重连 | 乱序率、丢失率、重复率 | 均为0 | 任一>0 |
| POC-03 | Unity SDK 集成成本 | Unity 2022 LTS | 从0到收发消息计时 | 集成耗时、代码行数 | ≤1人日 | 超出预期3倍 |
| POC-04 | Unreal SDK 集成成本 | UE5 | 同上 | 同上 | ≤2人日 | 超出预期3倍 |
| POC-05 | Laser HUD 低干扰性 | 桌面环境+游戏内 | 用户测试，记录呼出/收起体验 | 呼出延迟、误触率、主观干扰评分 | 呼出<200ms，干扰评分达标(需先定基线) | 明显干扰游戏操作 |
| POC-06 | 全屏/无边框游戏 HUD 兼容 | Windows全屏独占+无边框 | 呼出HUD并操作，检查焦点/光标状态 | 输入异常次数 | 0次异常 | 出现输入错乱 |
| POC-07 | 反作弊兼容性 | 集成EAC/BattlEye测试环境 | 部署Engine Plugin方案，运行反作弊检测 | 是否触发封禁/警告 | 0次误报 | 任何误报 |
| POC-08 | K3s 节点故障恢复 | 3节点集群 | 强制下线1节点 | Pod重调度时间、会话恢复率 | 服务不中断（客户端自动重连成功） | 数据丢失或长时间不可用 |
| POC-09 | NATS/PostgreSQL 故障恢复 | HA配置 | 主节点故障切换 | 故障切换时间、消息丢失数 | RTO<候选目标，丢失0 | 超出目标或丢失>0 |
| POC-10 | 重连风暴 | 10K+模拟客户端 | 同时断线后同时重连 | Gateway CPU、成功率、恢复时间 | 成功率≥99%，恢复时间<候选目标 | 服务雪崩 |

## 48. Acceptance Criteria

各功能的验收标准见第10-32章各需求表格 Acceptance Criteria 列（篇幅所限部分需求以 Priority 简表呈现，详细 AC 在实施阶段的详细设计文档中逐条展开，须保留可追溯至本 SRS 的 ID）。

## 49. SLO Candidate Table

**以下全部为 Candidate Target，未经 Benchmark 验证，禁止当作已确认 SLA 使用：**

| 指标 | Candidate Target |
|---|---|
| Availability (IM Core) | 99.9%/月 |
| Message Latency P50/P95/P99 | 50ms / 150ms / 400ms（同区域） |
| Connection Success Rate | ≥99.5% |
| Reconnect Time | <2s（正常网络） |
| Message Loss Rate | 0（Core 路径） |
| Duplicate Rate | 0（幂等去重后） |
| Ordering Error Rate | 0 |
| Presence Latency | <1s |
| SDK Memory Usage | <20MB（移动端）Candidate |
| HUD CPU Usage | <2%（Laser态） Candidate |
| HUD GPU Usage | <1%（Laser态） Candidate |
| HUD Memory Usage | <50MB Candidate |

## 50. ADR Candidates

| ADR | 主题 | Decision Status |
|---|---|---|
| ADR-001 | Event Bus 选型（NATS JetStream vs Kafka） | Candidate，倾向 NATS（运维成本），Needs PoC |
| ADR-002 | Message 存储模型（单表+分区 vs 按会话分片） | Needs PoC |
| ADR-003 | Message Ordering 实现（Sequence in PG vs 分布式ID+Redis计数器） | Needs PoC |
| ADR-004 | WebSocket vs QUIC 默认协议 | Confirmed：WebSocket为默认，QUIC为可选增强 |
| ADR-005 | Extension 隔离方式（进程级 vs WASM 沙箱） | Needs PoC，安全权重高 |
| ADR-006 | 游戏身份联邦协议（自研 vs 标准OIDC扩展） | Candidate，倾向标准OIDC+自定义Claim |
| ADR-007 | HUD 架构（独立进程 overlay vs 引擎内渲染插件） | Needs PoC（POC-06/07） |
| ADR-008 | C ABI 必要性与范围 | Candidate，倾向"仅Unity/Unreal/原生游戏需要，Web/Godot走原生协议" |
| ADR-009 | Graph Database 是否引入 | Candidate，倾向V2+按需，非MVP |
| ADR-010 | 多区域部署模型 | Needs PoC，非MVP范围 |

## 51. Risk Register

| Risk ID | 描述 | 概率 | 影响 | 缓解 | PoC | Owner | Status |
|---|---|---|---|---|---|---|---|
| RISK-01 | 反作弊系统误判HUD/Overlay为作弊 | 中 | 高 | 优先非侵入方案，避免Hook | POC-07 | HUD架构师 | Open |
| RISK-02 | Overlay在不同图形API下兼容性碎片化 | 高 | 中 | 分级支持策略，明确不支持清单 | POC-06 | 图形工程师 | Open |
| RISK-03 | 100K+连接下Gateway性能不达标 | 中 | 高 | 提前压测，水平扩展设计 | POC-01 | 后端架构师 | Open |
| RISK-04 | 消息乱序/重复在高并发下出现 | 中 | 高 | Sequence+幂等键强约束 | POC-02 | IM Core团队 | Open |
| RISK-05 | 重连风暴导致雪崩 | 中 | 高 | 限速+渐进式Resume | POC-10 | SRE | Open |
| RISK-06 | 游戏身份被伪造 | 中 | 高 | 强制Server-to-Server Token Exchange | 安全审查 | 安全团队 | Open |
| RISK-07 | Extension故障拖垮Core（隔离失效） | 低 | 极高 | 强制异步边界+资源配额 | 架构评审 | 架构师 | Open |
| RISK-08 | 多租户数据隔离失效 | 低 | 极高 | 行级安全+独立密钥 | 安全测试 | 安全团队 | Open |
| RISK-09 | 消息/媒体存储增长失控 | 中 | 中 | Retention策略+分层存储 | 容量规划 | SRE | Open |
| RISK-10 | Moderation不足导致垃圾/骚扰泛滥 | 中 | 中 | MVP即含基础Flood/Block/Mute | 无需PoC | 产品 | Open |
| RISK-11 | 跨平台SDK维护成本失控（Unity/Unreal/Godot/C ABI并行） | 高 | 中 | C ABI核心+薄封装层策略 | POC-03/04 | SDK团队 | Open |
| RISK-12 | K3s有状态负载（PG/NATS/MinIO）运维复杂度 | 中 | 中 | 优先Operator化管理 | POC-08/09 | SRE | Open |
| RISK-13 | AI隐私/Context泄露跨会话边界 | 低 | 高 | 权限隔离+Context Capsule设计 | 安全审查 | AI团队(V2) | Open |
| RISK-14 | LLM Token成本失控 | 中 | 中 | Context Capsule削减+限流 | 成本监控(V2) | AI团队 | Open |
| RISK-15 | 团队为技术先进性过度设计（Graph/Vector/微服务） | 高 | 中 | 本SRS明确非目标+MVP边界 | 架构评审 | 全体 | Mitigated by design |

## 52. Open Issues

| ID | 分类 | Question | Why It Matters | Options | Recommended | Blocking? |
|---|---|---|---|---|---|---|
| ISS-001 | Architecture | Extension隔离用进程级还是WASM沙箱？ | 影响安全/性能/开发体验 | 进程/WASM/容器 | Needs PoC，倾向进程级起步 | 否（V1前需定） |
| ISS-002 | Game | 是否MVP即支持Unreal，还是先Unity验证H2？ | 影响MVP周期 | 仅Unity / 双引擎并行 | 仅Unity，Unreal入V1 | 是（MVP范围） |
| ISS-003 | HUD | Laser HUD在Linux/macOS的Overlay技术路线是否与Windows一致？ | 影响HUD架构统一性 | 独立窗口统一方案 vs 平台特化 | 独立窗口优先，平台特化按需 | 否 |
| ISS-004 | Security | Token Exchange的Server Secret如何在私有部署下轮换？ | 影响安全运维 | 手动/自动轮换API | 需产品与安全团队共同确认 | 是（V1前） |
| ISS-005 | Infrastructure | NATS JetStream vs Kafka最终选型？ | 影响运维复杂度与吞吐 | 见ADR-001 | 需PoC数据 | 是（架构冻结前） |
| ISS-006 | AI | Context Capsule的Vector检索默认开启还是租户可选？ | 影响默认部署复杂度与成本 | 默认关闭，租户显式开启 | 默认关闭 | 否（V2范围） |
| ISS-007 | Operation | 100K+并发的具体SLO数字尚无实测基线 | 影响承诺给客户的SLA | 待POC-01/10数据 | 压测后回填Confirmed | 是（对外承诺前） |
| ISS-008 | Business | 计费模型（按连接数/MAU/消息量）尚未定义 | 影响多租户计量设计 | 待产品/商业团队确认 | 需产品负责人决策 | 否（MVP不涉及计费） |
| ISS-009 | Product | 好友系统"关闭开关"的具体UX与API契约未定 | 影响IM-REL-002落地 | 需详细设计阶段展开 | — | 否 |

## 53. Requirements Traceability Matrix（节选）

| Product Goal | Requirement | Use Case | Architecture Obligation | PoC/Test | Acceptance |
|---|---|---|---|---|---|
| G1 可靠IM | IM-FR-002/003/004/005 | UC-04 | Core独立数据模型+Sequence+幂等 | POC-02 | 乱序/丢失/重复率=0 |
| G2 低成本嵌入 | GAME-FR-001~004, SDK-FR-001~003 | UC-01 | C ABI+薄引擎绑定 | POC-03/04 | 集成耗时达标 |
| G3 低干扰访问 | HUD-FR-001~005, HUD-IX-001~005 | UC-03 | HUD为Client Layer，服务器无感知 | POC-05/06 | 无输入异常，干扰评分达标 |
| G4 扩展隔离 | EXT-FR-002/003, AI-FR-006/007 | UC-05/06 | 异步事件边界+资源隔离 | 架构评审+混沌测试 | Extension故障不影响Core |
| G5 弹性伸缩 | OPS-NFR-*, im-gateway/im-core边界 | UC-07 | Scaling/Failure Boundary拆分原则 | POC-01/08/09/10 | 达到候选SLO |
| G6 多租户 | TEN-FR-001~005 | UC-08 | 行级隔离+独立密钥/配额 | 安全测试 | 租户数据零泄露 |

（完整矩阵覆盖全部编号需求，将在详细设计阶段作为独立可维护表格产出，避免SRS正文过度膨胀。）

## 54. Glossary

- **IM Core**：不依赖任何垂直扩展的稳定即时通信内核。
- **Extension Runtime**：承载Game/AI/Work等可插拔能力的统一运行时。
- **Laser/Peek/Chat/Full Client**：HUD四层交互模型的四个渐进层级。
- **Context Capsule**：为AI动态组装、权限受限的最小必要上下文包。
- **Authoritative Source**：某类状态（如Guild成员关系）的唯一可信来源，通常是游戏服务器。
- **Candidate Target**：尚未经过Benchmark验证的指标，区别于Confirmed。

---

# 第39章 自我审查（Architecture / IM / Game / Security / Product Review）

以下审查已将发现的问题**直接修订进正文**（各章节的红线标注、架构约束、Candidate标记均为审查产物），此处仅总结审查结论：

1. **Architecture Review**：初稿曾倾向为游戏概念（Guild/Match）在Core Schema中增加字段，已改为强制metadata模式（IM-CONV-002, GAME-SOC-001）；识别出Extension同步调用风险，已加入EXT-FR-003/PERF红线；微服务拆分理由已收紧为四类边界（第39章K3s部分）。
2. **IM Expert Review**：补充幂等键、Sequence单调性、离线增量同步、大规模Presence限频（IM-PRES-004）等此前遗漏点。
3. **Game Integration Review**：明确Server Authentication为红线（GAME-ID-003/SEC-NFR-005），补充大型Guild/World Channel的Fan-out退化策略（GAME-SOC-004）。
4. **Security Review**：强化Token Exchange不可绕过设计，补充审计与ABAC要求；AI跨会话权限隔离列为红线（AI-FR-006）。
5. **Product Review**：删除了初稿中"默认引入Graph DB"与"AI优先于HUD"的倾向性描述，明确Vector/Graph为V2增强能力而非MVP依赖（第40章、ADR-009、RISK-15）；MVP范围收紧为仅Unity+Guest/JWT身份+无AI/Work。

---

# Top 10 Architecture Decisions（进入基本设计前必须确认）

1. Extension隔离机制：进程级 vs WASM沙箱（ADR-005）。
2. Event Bus选型：NATS JetStream vs 其他（ADR-001）。
3. Message Ordering与存储分区策略（ADR-002/003）。
4. C ABI的最终范围——是否所有非Web/Godot引擎都必须走C ABI（ADR-008）。
5. HUD Overlay在Windows各图形API下的具体实现路线（ADR-007，依赖POC-06/07结果）。
6. 微服务初始拆分边界是否维持五个逻辑服务，还是进一步合并为单体起步（第39章原则的具体落地）。
7. Token Exchange与Server Secret的密钥管理与轮换机制（呼应ISS-004）。
8. 多租户数据隔离的具体技术手段（PG行级安全 vs 独立Schema vs 独立库）。
9. Presence大规模广播的聚合/限频算法细节。
10. K3s上PostgreSQL/NATS/MinIO是否使用Operator托管有状态服务。

# Top 10 Product Decisions（产品负责人必须确认）

1. MVP是否严格排除Unreal SDK，仅用Unity验证H2（呼应ISS-002）。
2. 好友系统"可关闭"开关的产品形态与哪些游戏品类默认关闭（IM-REL-002）。
3. Laser HUD的视觉语言基线（颜色、强度上限）需要设计团队定稿后才能定义"低干扰"验收标准。
4. 计费模型方向（连接数/MAU/消息量/混合）（ISS-008）。
5. 私有化部署是否MVP即支持，还是先SaaS验证。
6. Moderation的默认严格程度（默认宽松+租户自定义 vs 默认严格）。
7. AI Extension的第一个落地场景优先级（Observer摘要 vs Bot助手 vs Moderation）。
8. 数据保留默认策略（尤其Guild频道消息默认保留多久）。
9. Guest账号的默认能力边界（是否能加好友、是否能被搜索）。
10. 是否第一阶段就对外提供SaaS Dashboard，还是仅API/SDK+文档。

# Top 10 Technical Risks（最可能导致失败/延期/大规模返工）

1. 反作弊系统误判Overlay/HUD导致游戏客户无法使用（RISK-01）。
2. 100K+长连接下Gateway性能不达标，需要架构重做（RISK-03）。
3. 消息顺序/幂等在高并发下失效，损害IM Core可信度（RISK-04）。
4. Extension隔离设计不彻底，AI/Work故障拖垮基础聊天（RISK-07）。
5. 跨引擎SDK（Unity/Unreal/C ABI）维护成本远超预期，拖慢游戏合作方接入（RISK-11）。
6. 游戏身份伪造/Token被提取导致大规模作弊或安全事件（RISK-06）。
7. 重连风暴/登录风暴在大型游戏上线日击穿系统（RISK-05）。
8. K3s上有状态组件（PG/NATS/MinIO）运维复杂度被低估，导致故障恢复不达标（RISK-12）。
9. 团队在压力下为"技术先进性"引入不必要的Graph/Vector/过度微服务化，违反IM-First原则（RISK-15）。
10. 多租户隔离存在漏洞，导致跨客户数据泄露，造成信任与合规危机（RISK-08）。
