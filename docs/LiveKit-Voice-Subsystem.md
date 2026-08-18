# LiveKit 游戏 IM 实时语音子系统 技术调查与需求定义书

版本：v0.1 Draft ｜ 关联文档：`docs/SRS.md`（可嵌入式游戏 IM 平台 SRS）
调查范围声明：本文档中所有可能随 LiveKit 版本变化的信息（License、SDK 支持范围、部署约束）均基于 2026 年公开资料检索结果，标注来源；无法从公开资料确认或涉及法律判断的条目一律标记 **LEGAL-REVIEW**，不作为最终定论。性能数字未经压测的一律标记 **BENCHMARK REQUIRED**。

---

## 1. Executive Summary

LiveKit 是一个 Apache-2.0 许可、可自托管的 WebRTC SFU（Selective Forwarding Unit）实时媒体基础设施，包含 Server、多语言 Client/Server SDK、Egress/Ingress/Agents 等周边组件。它**不理解** IM、好友、Guild、Party、Match 等业务概念，也不提供空间语音（Proximity Voice）的兴趣管理能力。

结论先行（详见第 48 章）：**ADOPT WITH CONDITIONS**——将 LiveKit 严格限定为 Media Plane，业务语音权限、Voice Scope、空间兴趣管理由本项目自建的 Rust `voice-control-service` 承担；K3s 部署需采用 hostNetwork 专用 Media Node 池，不得与 IM Core 服务混布；Unreal/Console SDK 现状不成熟，需要自建 Adapter 层或延后支持，作为条件写入 MVP 范围与风险登记表。

## 2. LiveKit Investigation（架构与组件调查）

| 组件 | Purpose | Required? | Game IM Use Case | 性能影响 | 部署影响 | License |
|---|---|---|---|---|---|---|
| Room | 媒体会话容器 | 必需 | 映射为 Voice Scope 的媒体载体，非 IM Conversation | 中 | 无状态可扩展 | Apache-2.0 |
| Participant | Room 内媒体参与者 | 必需 | 映射自 Voice Identity，非 IM User | 低 | — | Apache-2.0 |
| Track/Publication/Subscription | 媒体流发布/订阅 | 必需 | 承载语音，订阅关系=Audible Set 的执行层 | 高（Egress带宽核心） | — | Apache-2.0 |
| SFU | 媒体转发核心 | 必需 | — | 高 CPU/带宽 | hostNetwork，单 Pod 单节点（见第26章） | Apache-2.0 |
| Signaling (WebSocket) | 建连/控制信令 | 必需 | Token 校验入口 | 低 | 可走常规 LB (7880端口) | Apache-2.0 |
| ICE/STUN/内置 TURN | NAT 穿透 | 必需 | — | 中 | 需公网 UDP 端口范围 | Apache-2.0 |
| Data Channel | 低延迟数据消息 | 可选 | **不用于 IM 消息**，仅用于语音层控制信令（如说话状态） | 低 | — | Apache-2.0 |
| Simulcast/Dynacast | 多分辨率/按需转发 | 可选（视频场景为主） | 游戏语音场景价值有限（无视频） | — | — | Apache-2.0 |
| Adaptive Stream | 自适应码率 | 可选 | 弱网语音有一定价值 | — | — | Apache-2.0 |
| Egress | 录制/推流导出 | 可选 | 未来 Voice Recording（见第33章），MVP 不启用 | 独立进程，额外资源 | 独立部署 | Apache-2.0 |
| Ingress | 外部流接入 | 不需要 | 游戏语音无此场景 | — | — | Apache-2.0 |
| Agents | LiveKit 的 AI Agent 框架 | 不采用 | 与本项目 AI Extension 定位重叠，**不使用 LiveKit Agents，AI Voice 由本项目 AI Extension 自行编排**（见第38章） | — | — | Apache-2.0 |

来源：[livekit/livekit GitHub](https://github.com/livekit/livekit), [LiveKit Self-hosting overview](https://docs.livekit.io/transport/self-hosting/)

## 3. License（LEGAL-REVIEW 标记项）

- LiveKit Server（`livekit/livekit`）与主流 Server/Client SDK 仓库（如 `server-sdk-kotlin`）均采用 **Apache License 2.0**：允许商业使用、修改、私有部署、闭源产品集成，无需公开己方源码，无 SaaS Clause（非 AGPL/SSPL/BUSL 类限制型许可）。来源：[GitHub server-sdk-kotlin LICENSE](https://github.com/livekit/server-sdk-kotlin/blob/main/LICENSE), [LiveKit Self-hosting overview](https://docs.livekit.io/transport/self-hosting/)。
- 自托管无授权费、无按分钟计费、无参与人数上限，仅承担自有服务器成本。来源同上。
- **LEGAL-REVIEW-01**：需逐一确认本项目实际使用的每个 SDK 仓库（Unity/Unreal/Rust/各 Server SDK）License 头是否一致为 Apache-2.0，不能仅以 Server 仓库许可证代表全部子仓库——历史上部分开源项目存在个别子模块许可证不一致的情况，需要在详细设计阶段由法务/合规复核每个被引入仓库的 LICENSE 文件原文。
- **LEGAL-REVIEW-02**：LiveKit Cloud（SaaS 托管服务）本身不是开源软件，若未来任何环节使用 LiveKit Cloud 而非自托管，需重新评估其服务条款（不在本次调查范围，本项目结论基于**自托管**路径）。
- **LEGAL-REVIEW-03**：修改 LiveKit 源码用于内部商业产品在 Apache-2.0 下允许，但若计划对外分发修改版二进制/源码，需保留原始版权声明与 NOTICE 文件，属于常规 Apache-2.0 合规义务，建议法务出具一次性合规清单而非逐次判断。

**结论（非法律意见，供工程决策参考）**：现有公开信息支持"作为内部组件用于闭源商业 IM，无需开源本项目代码"的使用方式；最终以法务复核为准。

## 4. Fit/Gap Analysis

| 能力 | LiveKit 已提供 | 差距/需自建 |
|---|---|---|
| 媒体转发（SFU）、ICE/TURN、Opus 编解码传输 | ✓ | — |
| Room/Track 级权限（Publish/Subscribe grant） | ✓（Token 内嵌 grants） | 业务语义映射（谁能进入哪个 Room）需自建 |
| Guild/Party/Match/Proximity 等游戏社交语义 | ✗ | 自建 VoiceScope + voice-control-service |
| 空间兴趣管理（谁能听见谁，基于坐标） | ✗（无原生 AOI） | 自建 Spatial Voice Service 或复用游戏 AOI |
| 多租户/Tenant 隔离、审计、计费 | ✗ | 自建（依托 IM Core 多租户模型） |
| Unity 官方 SDK | ✓（活跃维护） | — |
| Unreal 官方 SDK | ✗（尚无官方稳定版，仅有基于 Rust SDK 讨论） | 需 Adapter Layer 或延后（ADR-VOICE-007） |
| Godot SDK | ✗（无官方） | 需自建（C ABI + GDExtension，ADR REQUIRED） |
| AI Agent 编排 | ✓（LiveKit Agents 框架） | **不采用**，避免与本项目 AI Extension 架构重复（见第38章） |

## 5. System Context

```
                 IM / Game Platform
                        │
                Voice Control Plane（Rust, 自建）
                        │
        ┌───────────────┼───────────────┐
        │                               │
   LiveKit (Media Plane)             coturn（可选/按需）
        │
        ▼
   RTP/RTCP over UDP（客户端 ⇄ Media Node）
```

**边界铁律**：LiveKit 不持有 IM User、Conversation、Guild、Tenant 等业务实体；这些实体只存在于 IM Core / Game Extension / Voice Control Service 中，LiveKit 仅接收"已授权的 Room + Participant + Grant"指令。

## 6. Control Plane / Media Plane 划分

| 职责 | Control Plane（自建 Rust 服务） | Media Plane（LiveKit） |
|---|---|---|
| 用户/租户/Guild/Party 身份与权限 | ✓ | ✗ |
| Voice Scope → Room 映射决策 | ✓ | ✗ |
| Token 签发（含 grants：canPublish/canSubscribe） | ✓（调用 LiveKit Server API 生成） | 校验并执行 |
| Room 生命周期（创建/销毁/TTL） | ✓ 发起 | 执行 |
| 订阅关系动态调整（如空间语音的 unsubscribe） | ✓ 决策 | 执行 |
| RTP 转发、Opus 编解码传输、ICE/TURN | ✗ | ✓ |
| Mute/Kick/Ban 的业务判定 | ✓ 判定 | 执行（通过 Server API） |
| 审计日志、Rate Limit、Tenant 隔离 | ✓ | ✗ |

## 7. Voice Domain Model

```
Tenant → Game → Environment → IM User → Device Session
                                   │
                                   ▼
                            Voice Identity（LiveKit Participant Identity）
                                   │
                                   ▼
                         VoiceScope（Direct/Party/Guild/Match/Proximity/...）
                                   │
                                   ▼
                    Voice Control Service 决策 Room/Grants
                                   │
                                   ▼
                          LiveKit Room / Track / Subscription
```

**Core 数据模型零污染原则**：`Room.name`、`Room.metadata` 中可携带 `voice_scope`、`tenant_id`、`game_id` 等引用字段，但 IM Core 数据库 Schema 不新增语音专有字段（呼应 SRS `IM-CONV-002` 架构约束）。

## 8. Identity Mapping

```
IM User (UUID) + Device Session
        │
        ▼
Voice Identity = f(tenant_id, game_id, im_user_id, session_id)
        │
        ▼
LiveKit Participant Identity（string，全局在 Room 内唯一）
```

设计要点：

- **多设备**：一个 IM User 若允许多设备同时语音（通常不允许，避免回声/双发），Voice Control Service 强制单一活跃 Voice Session，新设备加入时先使旧 Participant 断开（`RemoveParticipant`）。
- **重复登录/断线重连**：Voice Identity 中嵌入 Session 版本号，旧 Token 在新 Session 建立后立即失效（服务端主动 Revoke，而非仅依赖客户端行为）。
- **Guest**：沿用 IM Core 的 Guest Identity，Voice Identity 派生规则一致，无特殊处理。
- **Token 生命周期**：短期 Token（建议 ≤10 分钟，**BENCHMARK REQUIRED** 确认对高频重连场景的影响），仅用于单次 Room 加入，不可跨 Room 复用。
- **Token Replay/泄露防护**：Token 内嵌 Room 名、grants、过期时间并由 Voice Control Service 私钥签名；LiveKit Server 校验签名与过期时间；即使 Token 被截获，超时后自动失效，且不允许变更 grants（防止客户端伪造更高权限）。
- **Room Permission**：Publish/Subscribe grants 完全由 Voice Control Service 在签发 Token 时决定，客户端 SDK 不暴露"请求更高权限"的 API。

## 9. VoiceScope

```
enum VoiceScope {
    Direct(user_a, user_b),
    Party(party_id),
    Team(match_id, team_id),
    Guild(guild_id, channel_id),
    Clan(clan_id),
    Match(match_id),
    Lobby(lobby_id),
    Proximity(world_id, region_id),
    Spectator(match_id),
    WorldEvent(world_id, event_id),
    Custom(extension_id, scope_key),
}
```

`VoiceScope` 是 Voice Control Service 内部枚举，**不直接写入 LiveKit**。Voice Control Service 负责将其映射为 `(Room Name, Room Metadata, Grants)`。同一 VoiceScope 可能对应一个 Room（如 Party），也可能对应"逻辑分组 + 动态订阅子集"（如 Proximity，见第14章）——映射策略按 Scope 类型独立决策，禁止用单一固定公式处理所有 Scope。

## 10. Party Voice

```
Party State (Game Server, authoritative)
        │ event: member_joined / member_left / party_disbanded
        ▼
Game Extension（事件订阅）
        │
        ▼
Voice Control Service
        │  ├─ create/reuse Room("party:{party_id}")
        │  ├─ issue Token(grants: publish+subscribe within room)
        │  └─ on member_left → RemoveParticipant
        ▼
LiveKit Room
```

- 自动 Join/Leave 完全由 Game Server 权威事件驱动，客户端不能自行调用"加入语音"绕开 Party 状态校验。
- Party Owner 拥有的 Mute/Kick 为**业务权限**，由 Voice Control Service 校验 Owner 身份后代为调用 LiveKit `MutePublishedTrack`/`RemoveParticipant`，客户端 SDK 不直接持有 LiveKit Server API 密钥。
- Self Mute（本地静音）与 Server Mute（服务端强制）分离实现：前者仅客户端本地停止发布 Track，后者服务端调用 API 强制 Mute，二者状态需在 UI 层面区分显示。
- Party 解散 → Room 立即销毁（不保留空 Room，避免资源泄漏）。

## 11. Guild Voice

**Guild Members ≠ Simultaneous Voice Participants**：一个 Guild 可能有 1000+ 成员，但同时说话/在线语音的通常是几十人，因此**不能默认 1 Guild = 1 LiveKit Room**。

三种模型比较：

| 模型 | 说明 | 优点 | 缺点 | 适用场景 |
|---|---|---|---|---|
| A. 单 Room 直映射 | 1 Guild = 1 Room | 实现简单 | 大 Guild 时 Room 内 Participant 数过大，Egress 带宽爆炸（见第21章） | 小型 Guild（<100人）或频道人数被限制的场景 |
| B. 多子频道 Room | Guild → 多个 Voice Channel（如 Raid/Officer/General），每个 Channel 独立 Room，成员显式加入具体 Channel | 贴近真实使用习惯，天然限制单 Room 规模 | 需要频道管理 UI/权限 | **推荐**：中大型 Guild |
| C. 动态临时 Room | 仅在有人发起语音时创建 Room，空闲自动销毁 | 资源利用率最高 | 增加 Room 生命周期管理复杂度 | Raid/临时小队场景，可与模型B组合 |

**推荐**：模型 B（子频道）为默认，模型 C（临时 Room）用于 Raid Channel 等临时场景，二者组合使用（ADR-VOICE-005）。

## 12. Match Voice

权限矩阵示例（一局 5v5 对战 + 观战 + GM）：

| Publisher \ Subscriber | Team A | Team B | Spectator | GM |
|---|---|---|---|---|
| Team A | ✓ | ✗ | ✓（延迟/受限，防作弊） | ✓ |
| Team B | ✗ | ✓ | ✓ | ✓ |
| Spectator | ✗ | ✗ | ✓ | ✓ |
| GM | ✓ | ✓ | ✓ | ✓ |

实现方式：为每个 Team 建立独立 Room（`match:{id}:teamA`），Spectator 加入只读订阅 Room（`match:{id}:spectator`，聚合双方经服务端策略处理后的流，或仅订阅公共播报），GM 通过多 Room 加入拥有全局订阅权限。**Grants 由 Voice Control Service 在 Token 签发时静态确定，客户端无法通过修改 SDK 调用自行订阅未授权 Track**——LiveKit Server 端强制校验 Token grants，天然阻止"偷听对方 Team"。

## 13. Proximity Voice

```
PlayerPosition { player_id, world_id, x, y, z, orientation }
        │
        ▼
Spatial Interest（AOI，见第14章）
        │
        ▼
Audible Set: { B: full, C: attenuated, D: weak, E: unsubscribe }
        │
        ▼
Voice Control Service → 调整该玩家在 Room 内的 Subscription 集合
```

关键设计：Audible Set 的变化频率通常远高于 Party/Guild 场景（玩家持续移动），因此**订阅调整必须走轻量增量 API（`UpdateSubscriptions`），不得每次都重新签发 Token 或重建 Room**。

## 14. 空间语音架构：不把空间计算交给 LiveKit

调查结论：LiveKit 原生**不提供**基于三维坐标的兴趣管理/空间音频能力，其 Room/Track/Subscription 模型是通用的，不理解"距离"。因此空间语音的兴趣计算必须自建或复用游戏已有能力。

```
Game World（游戏服务器权威位置数据）
        │
        ▼
AOI / Interest Management（优先复用游戏服务器已有实现）
        │
        ▼
Audible Set（Spatial Voice Service 计算，若游戏服务器已有 AOI 则直接复用其结果）
        │
        ▼
Voice Control Service（转换为 LiveKit Subscription 变更）
        │
        ▼
LiveKit
```

**原则（第12章要求）**：优先复用游戏服务器已有的 AOI/Interest Management（Grid/Quadtree/Octree/BVH/Spatial Hash 等，游戏后端通常已为战斗/同步实现过此类系统）；仅当游戏方无此能力时，才考虑由 Spatial Voice Service 自建一个轻量 Grid-based AOI（复杂度最低，足以满足语音场景对精度的低要求——语音兴趣半径远大于位置同步精度需求）。**不要在 MVP/V1 阶段自建 Quadtree/Octree/BVH 等复杂空间数据库**，这是过度工程（Overengineering，第47章 Review 5 明确删除项）。

## 15. Spatial Audio（网络兴趣 vs 声音空间化）

- **Network Interest（谁能听见谁）**：服务端职责，第14章已覆盖，由 Voice Control Service 通过 LiveKit Subscription 控制。
- **Audio Spatialization（听起来从哪个方向、多远）**：**客户端职责**，LiveKit 传输的是标准 Opus 音频流（无方向信息），方向感由客户端引擎的音频系统基于已知的对方位置渲染。
  - Unity：内置 Audio Source 3D 空间化，或 Unity Spatializer SDK。
  - Unreal：内置 Audio Component 3D Attenuation，或 Steam Audio 插件。
  - FMOD/Wwise：作为音频中间件，需要将 LiveKit 输出的 PCM/Opus 解码后的音频流路由进其 3D 音效引擎（**ADR REQUIRED**：需要验证 LiveKit Unity/Unreal SDK 是否暴露原始 PCM buffer 供第三方音频引擎接管，而非只能通过引擎默认 AudioSource 播放）。

## 16. Opus 音频 Profile 建议

LiveKit 音频传输基于标准 WebRTC 音频栈，Opus 为默认编解码器，支持可变比特率、FEC（前向纠错）、DTX（静音检测降码率）等标准 Opus 能力（**BENCHMARK REQUIRED**：LiveKit 各 SDK 版本对 FEC/DTX 参数的可配置粒度需在选定版本上实测验证，不同版本可能有差异）。

按场景建议的候选 Profile（**均为 Candidate，需 VOICE-BENCH-001 验证**）：

| 场景 | 声道 | 采样率 | 目标码率 | FEC | DTX | 备注 |
|---|---|---|---|---|---|---|
| Competitive Game（低延迟优先） | Mono | 48kHz | 16-24kbps | 开启 | 开启 | 竞技场景强调低延迟与抗丢包，非音质 |
| MMO（大规模 Party/Guild） | Mono | 48kHz | 16kbps | 开启 | 开启 | 控制总 Egress 带宽（第21章） |
| Casual Game | Mono | 48kHz | 24-32kbps | 开启 | 开启 | 平衡音质与带宽 |
| Mobile Game | Mono | 24-48kHz | 12-16kbps | 开启（弱网关键） | 开启 | 移动网络丢包/切换频繁 |
| High Quality Social（非竞技，小规模） | Stereo可选 | 48kHz | 32-64kbps | 可选 | 可选 | 面对面/小型语音房间，音质优先 |

## 17. 网络问题与 ICE/STUN/TURN 处理

| 问题 | 处理机制 |
|---|---|
| NAT/CGNAT | ICE 尝试 host/srflx（STUN）候选，失败则 relay（TURN） |
| 严格防火墙/UDP 全阻断 | LiveKit 支持 TURN over TLS（443端口）作为最后手段，模拟 HTTPS 流量穿透 |
| 高丢包 | Opus FEC + 客户端 Jitter Buffer；应用层不做额外重传（语音场景重传无意义，只会增加延迟） |
| 高 RTT | Adaptive Stream 调整；游戏场景应向玩家展示网络质量指示（呼应 HUD 集成，第28章） |
| Jitter | LiveKit/WebRTC 标准 Jitter Buffer 处理，客户端侧不需要额外实现 |
| 移动网络切换（Wi-Fi↔4G） | ICE Restart 机制，Voice Control Service 需支持客户端重连时快速复用/续期 Token（**避免每次网络切换都走完整鉴权流程**，这是 VOICE-POC-011 的核心验证点） |

## 18. coturn vs LiveKit 内置 TURN

| 维度 | LiveKit 内置 TURN | 独立 coturn |
|---|---|---|
| Deployment | 与 LiveKit Server 同进程/同 Helm Chart 内置，部署更简单 | 独立组件，需单独运维 |
| Performance | 与 LiveKit 共享节点资源，规模化后可能竞争 | 可独立扩缩容，专注 TURN 中继 |
| Security | 由 LiveKit 管理凭证生命周期，攻击面较小 | 需自行管理 TURN 凭证轮换 |
| Operations | 运维单一系统，复杂度低 | 多一套系统需监控告警 |
| Scaling | 跟随 LiveKit Media Node 扩缩容，可能不够精细 | 可根据 TURN 流量单独扩缩容（TURN 与 SFU 负载特征不同：TURN 是纯带宽转发，SFU 有编解码/路由开销） |
| Cost | 简单场景成本可控 | 大规模 TURN 流量场景可能更易做流量成本优化与多区域就近部署 |
| K3s | 无需额外 Pod | 需要额外 hostNetwork Pod 或独立部署 |
| Multi-region | 跟随 Media Node 区域分布 | 可独立在更多边缘位置部署，降低 TURN 中继延迟 |

**结论**：MVP/V1 阶段使用 **LiveKit 内置 TURN**，降低运维复杂度（呼应 SRS "初期禁止过度微服务化"原则）；当 TURN 流量占比与成本（第22章）超过预设阈值、或需要比 Media Node 更细粒度的地理分布时，再引入独立 coturn（ADR-VOICE-003，Needs PoC/运营数据）。

## 19. K3s 部署架构

```
K3s Cluster
├── Control Plane 命名空间（可与 IM Core 共存/水平扩展）
│   ├── voice-control-service（Rust, 标准 Deployment, 无需 hostNetwork）
│   ├── nats（IM Core 共用）
│   ├── postgres（IM Core 共用，语音元数据可共库不同 Schema）
│   ├── valkey（IM Core 共用或独立实例，视负载隔离需求）
│   └── observability（Prometheus/OTel Collector）
│
└── Media Plane 节点池（专用节点，taint 隔离，不与其他工作负载混布）
    └── livekit media node（hostNetwork: true，每节点单 Pod，占用节点公网 IP + UDP 端口范围）
```

**关键调查结论（第17章调查已确认）**：

- LiveKit Media Node **必须** `hostNetwork: true`，rtc.udp/tcp 端口由节点直接处理，Kubernetes Service 无法代理大范围 UDP 端口；因此**每节点仅能运行一个 LiveKit Pod**（来源：[LiveKit Kubernetes 文档](https://docs.livekit.io/transport/self-hosting/kubernetes/)、社区部署指南）。
- 仅 Signaling（7880端口）可走常规 LoadBalancer/Ingress；Media 流量直连节点公网 IP。
- **不支持 Serverless / 完全私有集群部署**（无公网出口的集群无法承载 Media Node）——若目标部署环境为纯内网 K3s（无公网 IP），需要额外设计 TURN-only 出口方案或放弃在该环境内跑 Media Node（标记 **ISS-VOICE-01**，见 Open Issues）。
- Pod Migration / 节点故障：Media Node 上的活跃 Room 状态与其所在 Pod/进程强绑定，节点故障会导致该节点承载的所有通话中断（无法无缝迁移正在进行的媒体会话），客户端需依赖 ICE Restart + 重新协商 Room 完成重连，这是**预期行为而非 bug**，需要在 Failure Model（第38章）与 SLO 中明确声明，而非假设"无感知迁移"。
- Session Affinity：Signaling 连接需要与其后续建立的 Media 连接保持一致的路由认知，Helm Chart 已处理此逻辑，自定义部署需谨慎复刻。
- Graceful Shutdown：需要给 Media Node 下线预留 Draining 时间（通知 Voice Control Service 触发客户端主动迁移到其他 Room 实例，或至少给出用户可感知的"语音即将重连"提示），不能简单依赖 K8s 默认 terminationGracePeriod。

## 20. Media Node 部署形态 ADR 输入

比较 A~E：

| 选项 | 说明 | 优点 | 缺点 |
|---|---|---|---|
| A. K3s 内部（hostNetwork Pod） | 与 Control Plane 同集群，专用节点池 | 统一运维、统一 Observability | 公网 IP/UDP 端口管理仍需手工介入，扩缩容不如无状态服务顺滑 |
| B. Dedicated Node（K3s 内但物理/云独立节点组） | 在 A 基础上进一步用 taint/nodeSelector 强隔离 | 故障域隔离更彻底 | 与 A 本质相同，仅是节点池策略 |
| C. Host Network 独立 VM（脱离 K3s 编排） | 手工/IaC 管理的裸机或云主机 | 对 UDP/公网 IP 控制力最强，运维模型最简单直接 | 失去 K8s 统一编排/自愈能力，需要独立的部署流水线 |
| D. 独立 VM + 自动化编排（如 Nomad/Terraform，非K8s） | — | 兼顾灵活性与自动化 | 引入第二套编排体系，违反"避免不必要复杂度"原则 |
| E. 混合模式（Control Plane 在 K3s，Media Node 视区域/规模选 A 或 C） | — | 兼顾统一管理与未来多区域灵活性 | 需要维护两种部署路径 |

**推荐（Candidate，ADR-VOICE-004）**：MVP/V1 采用 **选项 A**（K3s 内 hostNetwork 专用节点池），复用现有 K3s Helm 生态与 Observability；当出现明确的多区域/超大规模需求（第31章）且 A 方案暴露出编排开销大于收益的实证问题时，向 E 演进。不预先采用 C/D，避免过早引入第二套运维体系。

## 21. Bandwidth Model

**Ingress**（进入 SFU 的流量）：

```
Ingress = Σ(Publisher_i bitrate)
```

每个说话的玩家贡献一份 Opus 码率（见第16章 Profile），与订阅者数量无关。

**Egress**（SFU 转发出去的流量，通常是真正的瓶颈）：

```
Egress = Σ_room Σ_track (track bitrate × 该 track 的订阅者数)
```

**为什么 100 人 World Voice 不能无脑全互听**：若 100 人 Room 内假设平均同时有 10 人说话，每人 24kbps，若每个说话者被其余 99 人全部订阅：

```
Egress ≈ 10 × 24kbps × 99 ≈ 23.76 Mbps（单 Room）
```

这只是**一个** 100 人 Room 的持续 Egress；若干个这样的 Room 并存，SFU 出口带宽将迅速成为瓶颈，且绝大多数订阅是"玩家实际根本听不到/不需要听到"的无效转发（如两个相距很远的玩家）。**这正是 Interest Management / Subscription Control（第9/13/14章 VoiceScope + Proximity）存在的核心理由**：将全互听的 O(N²) 转发压力，通过业务规则（Party/Guild子频道/空间兴趣）收敛为远小于 N² 的实际订阅关系。

## 22. TURN Cost Model

```
TURN 流量成本 ≈ TURN Ratio × Voice Bitrate × Concurrent Users × Time × 单位流量成本
```

- `TURN Ratio`：实际走 TURN 中继而非直连/SFU 直通的连接占比。企业网络、严格 NAT、移动网络环境下该比例可能显著（**BENCHMARK REQUIRED**，公开资料中行业经验值区间较大，不同网络环境差异大，不采用未经验证的具体百分比）。
- 影响 TURN Ratio 的因素：客户端所在网络类型（家庭宽带/移动网络/企业网）、地理与 Media Node 部署密度的匹配程度、防火墙策略。
- 成本管理策略：优先让尽可能多连接走 ICE 直连（更贴近玩家的 Media Node 区域部署可改善直连率）；监控 `turn_ratio` 指标（第36章）作为持续优化输入，而非一次性估算后不再关注。

## 23. Game Voice SDK（对开发者的理想接口）

```
Voice.Initialize(config)
Voice.Login(im_token)
Voice.JoinParty(party_id)
Voice.LeaveParty()
Voice.JoinMatch(match_id, team_id)
Voice.SetMicrophone(device_id)
Voice.SetMute(bool)
Voice.SetOutputVolume(float)

// 空间语音（可选启用）
Voice.EnableProximity(world_id)
Voice.UpdateTransform(position, orientation)

// 状态回调
Voice.OnParticipantSpeaking(participant_id, level)
Voice.OnConnectionStateChanged(state)
```

设计原则：开发者只与业务语义交互（Party/Match/Mute/Transform），LiveKit Room/Track/Subscription 完全被 SDK 内部封装隐藏，呼应 SRS `SDK-FR-001`（五步接入模型）与 IM-First 的"开发者不理解内部微服务"原则。

## 24. Unity

- **官方 SDK 现状**：`livekit/client-sdk-unity` 为官方维护仓库，处于活跃开发状态；`client-sdk-unity-web` 专门覆盖 WebGL 平台，目前是较成熟的子集。主 SDK 目标是覆盖"所有 Unity 平台"，但截至调查时点，公开资料未明确列出各原生平台（Windows/macOS/Android/iOS）的逐一 GA 状态（来源：[client-sdk-unity GitHub](https://github.com/livekit/client-sdk-unity)）。
- **需在详细设计阶段逐项验证并标注状态**（不得猜测）：

| 平台 | 状态标注 |
|---|---|
| Windows (IL2CPP/Mono) | Unknown — 需 VOICE-POC-004 实测确认 |
| macOS | Unknown — 需 VOICE-POC-004 实测确认 |
| Android | Unknown — 需 VOICE-POC-004 实测确认，含麦克风权限/音频路由验证 |
| iOS | Unknown — 需 VOICE-POC-004 实测确认，含后台音频策略验证 |
| Console（PlayStation/Xbox/Switch） | **Requires Vendor Approval** — WebRTC 类中间件在主机平台通常需要平台方（Sony/Microsoft/Nintendo）审核与专用网络栈适配，公开资料未提供官方主机支持声明，不得假设可用 |

## 25. Unreal Engine

- **官方 SDK 现状**：截至调查时点，LiveKit **未提供正式发布的官方 Unreal SDK**；`rust-sdks` 仓库中存在关于"基于 Rust SDK 构建 Unreal 支持"的讨论（Issue，2024年9月），显示官方有意向但尚无稳定交付物（来源：[livekit/rust-sdks Issue #430](https://github.com/livekit/rust-sdks/issues/430)）。
- **结论**：Unreal 官方 SDK 状态为 **Requires Custom Port**。方案：
  1. 基于官方 `livekit/rust-sdks`（Rust 客户端 SDK）自建 **Adapter Layer**：Rust Core（Tokio + rust-sdks）→ C ABI → Unreal C++ Plugin，与本项目既有"Rust Core + C ABI 供多引擎复用"的架构原则天然契合（呼应 SRS `ADR-008`）。
  2. 该 Adapter Layer 工作量与维护成本需计入路线图，**不建议放入 MVP**（呼应 SRS 与本文档第45章，MVP 优先 Unity）。
- Blueprint 暴露：Adapter 完成后，建议在 C++ Plugin 层包一层 Blueprint Function Library，暴露第23章定义的简化 API，而非直接暴露底层 LiveKit 概念。

## 26. Godot

- 公开资料未见成熟的官方或社区 LiveKit Godot SDK。
- 方案：复用第25章同一 Rust Core（Rust + rust-sdks）→ C ABI → **GDExtension** 封装。
- **标记 ADR REQUIRED（ADR-VOICE-007 的子决策）**：是否值得为 Godot（当前非 P0 引擎，参考 SRS GAME-FR-002 优先级）投入独立 GDExtension 开发，还是延后至有明确客户需求时才启动，需产品侧确认优先级后再决策，不在本文档预先承诺交付时间。

## 27. 跨引擎 Voice Core（Rust Shared Core 可行性）

```
Rust Voice Core（基于官方 livekit/rust-sdks 封装业务语义：VoiceScope/Party/Proximity 等客户端状态机）
       │
      C ABI
       │
 ┌─────┼──────┐
 ▼     ▼      ▼
Unity Unreal Godot
(官方SDK)  (Adapter)  (Adapter)
```

- **价值**：避免 Party/Match/Proximity 等业务状态机在 Unity/Unreal/Godot 三套代码中重复实现，只需各引擎维护"薄层"绑定 + 音频设备/3D空间化对接（第15章已明确这部分必须留在引擎层，无法下沉到 Rust Core）。
- **前提**：仅当 Unreal/Godot 采用 Adapter 路线时，Rust Shared Core 才有意义；Unity 已有官方成熟 SDK，是否让 Unity 也切换到"Rust Core + C ABI"而非直接使用官方 Unity SDK，需权衡——**默认结论：Unity 继续使用官方 SDK（成熟度更高、社区支持更好），不强行统一到自建 Rust Core**，仅 Unreal/Godot 走 Rust Core 路线（避免"为了 Rust 而 Rust"，呼应第27章原始要求）。
- ADR-VOICE-008 记录该决策与复核条件（若未来官方 Unreal/Godot SDK 成熟发布，应重新评估是否放弃自建 Adapter）。

## 28. HUD 集成

```
┃ 🎙 Alice  (speaking, pulse)
┃ 🎙 Bob    (muted, static)
┃
┃ Party Voice
```

- Laser 层（呼应 SRS HUD-FR-003）新增语音状态表达：Voice Active（说话中，pulse）、Incoming Voice（有人开始说话，短促 glow）、Muted（静态低亮度图标）、Disconnected/Reconnecting（wave 效果）。
- **红线**：语音功能不得让 Laser 层退化为常驻大面积 Overlay（如常驻显示全部 Party 成员头像列表）；默认态仍应只有 2~4px 光轨 + 极简状态指示，完整说话者列表下沉至 Peek/Chat 层（呼应 SRS "Never In The Way"）。

## 29. Push-to-Talk

| 输入设备 | 实现要点 |
|---|---|
| 键盘/鼠标 | 需要 Global Hotkey（游戏可能独占键盘焦点），与 SRS `HUD-IX-003` 同样面临"不得扰乱游戏 Raw Input"约束 |
| 手柄 | 需与游戏自身手柄绑定协商（如 L3/R3 或肩键组合），避免冲突；建议开放绑定配置而非硬编码 |
| Hold 模式 | 按下发布 Track，松开立即停止发布（而非仅静音，减少无效 Ingress，呼应第21章带宽模型） |
| Toggle 模式 | 与 Voice Activation（自动检测说话）互斥，需在 UI 明确当前模式 |

## 30. Audio Device 责任边界

| 能力 | 归属 |
|---|---|
| Microphone/Output Device 枚举与选择 | OS + 引擎（Unity/Unreal Audio API），LiveKit SDK 通常提供设备选择接口封装 |
| Volume / Input Gain | 引擎/我们的 SDK 上层封装 |
| Noise Suppression / Echo Cancellation / AGC | **WebRTC 标准音频处理链**（LiveKit 底层基于 WebRTC，通常内置 APM——Audio Processing Module），需在选定 SDK 版本上确认具体开关粒度（**BENCHMARK REQUIRED**） |
| 3D 空间化 | 引擎/音频中间件（第15章已明确） |

## 31. AI Voice（Future Extension，非 MVP）

```
Voice → STT → AI（本项目 AI Extension） → TTS → LiveKit（作为一个 Participant 发布合成音频）
```

- AI 作为 Participant 加入 Room 是 LiveKit 原生支持的能力（其官方 Agents 框架即基于此模式），但**本项目不直接采用 LiveKit Agents 框架**，而是让 AI Extension（呼应 SRS 第27章）自行编排 STT/TTS/LLM 调用，仅将 LiveKit 当作媒体收发端点，避免两套 AI 编排体系并存。
- **红线**：AI Voice（NPC/翻译/助手）故障必须被隔离，不得影响同一 Room 内人类玩家之间的正常语音（AI Participant 崩溃 = 移除该 Participant，Room 内其他人不受影响，天然满足于 LiveKit 的 Participant 隔离模型）。

## 32. 实时翻译（Future，非 MVP）

```
Japanese Voice → STT → Translation → TTS → Chinese Player（语音）
                                   └→ 字幕（更低延迟，优先于 TTS）
```

字幕翻译路径（STT→Translation→字幕文本）延迟显著低于完整 TTS 语音合成路径，产品应**优先实现字幕翻译**作为 V2 早期能力，完整语音翻译（含 TTS）延后（呼应 SRS 非目标与 MVP 收紧原则）。

## 33. Moderation

- 基础能力：Mute/Kick/Ban 由 Voice Control Service 结合 IM Core 的 Moderation 权限模型执行（GM/Moderator 身份复用 IM Core，不在语音侧重建一套权限系统）。
- Report：举报语音行为时，若已开启 Egress 录制（见 Privacy 章节，默认关闭），可关联证据片段；若未开启录制，仅能记录举报上下文（时间、Room、参与者列表）。
- **AI-based Voice Moderation（STT+关键词/毒性检测）为增强扩展，不是基础能力依赖**——呼应 SRS `MOD-FR-004` 同一原则：基础 Moderation（Mute/Kick/Ban/Report）不依赖 AI。

## 34. Privacy

- **默认原则：不录音**。LiveKit Egress 具备录制能力，但默认不启用；仅当租户/游戏显式开启（如合规要求的证据留存场景）且已完成用户告知/同意流程时才启用 Egress 录制。
- STT/AI 处理同理：仅在用户/租户明确 Opt-in 的功能路径（如实时翻译）才会将语音流转交 AI 处理，默认关闭。
- 临时音频（Jitter Buffer/中转缓冲）不构成"留存"，仅在录制显式开启时才涉及 Retention 策略（呼应 SRS `PRIV-FR-002`）。

## 35. Security 威胁建模

| Threat | Attack Path | Impact | Mitigation | Verification |
|---|---|---|---|---|
| Fake Participant | 客户端伪造/复用他人 Voice Identity | 冒充身份说话/监听 | Token 由 Voice Control Service 签发并绑定 Identity+Room+grants，客户端无法自签 | 安全测试：尝试用篡改 Identity 的 Token 加入 Room 应被拒绝 |
| Token Theft | Token 在客户端内存/网络中被提取 | 短时间窗口内冒用 | 短生命周期 Token（第8章）+ 单 Room 单次有效 | 渗透测试：提取 Token 后超时/换 Room 应失效 |
| Token Replay | 重放已使用 Token | 重复加入/绕过限制 | 服务端记录已消费 Token（或 Room+Identity 唯一性约束），过期强制失效 | 安全测试 |
| Room Enumeration | 猜测/遍历 Room 名称探测活跃语音会话 | 信息泄露、未授权尝试加入 | Room 名称包含不可预测部分（如 UUID），加入仍需有效 Token 而非仅知道 Room 名 | 安全测试：无 Token 尝试加入应被拒绝 |
| Unauthorized Subscription | 客户端篡改 SDK 尝试订阅未授权 Track | 偷听（如第12章 Match 场景偷听对方 Team） | Grants 服务端强制校验，SFU 层拒绝未授权订阅请求，而非依赖客户端自律 | 安全测试：修改客户端订阅未授权 Track ID 应被拒绝 |
| Unauthorized Publish | 客户端伪造发布未获授权的 Track | 语音注入/骚扰 | canPublish grant 校验 | 安全测试 |
| Voice Spam/Audio Flood | 恶意用户高频发布垃圾音频/噪音 | 骚扰其他玩家 | Server Mute/Kick（第33章）+ 举报机制 + 速率限制（异常发布模式检测） | 运营监控 |
| Bot/DoS | 大量伪造连接请求耗尽 Signaling/TURN 资源 | 服务不可用 | Rate Limit（IM Core 统一网关层）+ Token 前置鉴权，未鉴权请求不触达 LiveKit | 压测（VOICE-POC-009相关） |
| TURN Abuse | 利用开放 TURN 作为匿名流量中继跳板 | 带宽被盗用、合规风险 | TURN 凭证短期化且与 Voice Session 绑定，禁止匿名 TURN 使用 | 安全测试 |

## 36. Observability

LiveKit 自身提供的指标（**需在选定版本上核实具体指标名称与 Prometheus Exporter 支持范围，BENCHMARK/VERIFY REQUIRED**）：通常包括 Room/Participant 数量、Track 发布/订阅数、Packet Loss、Bitrate 等媒体层指标（来源：官方文档一般提供 Prometheus metrics endpoint，需在实施阶段对照具体版本文档核实字段）。

本项目需自建补充的指标（Voice Control Service / 网关层）：

```
voice_connections
voice_connect_latency
voice_reconnect_rate
active_rooms
active_participants
published_audio_tracks
subscriptions
packet_loss        (若LiveKit未提供细粒度，需自行从WebRTC stats采集)
jitter
rtt
audio_bitrate
turn_ratio
sfu_ingress
sfu_egress
```

指标链路需能定位：`Game Client → Voice Control Service → LiveKit Signaling → LiveKit Media Node`，与 SRS 第37章 IM Core 的 OpenTelemetry 体系共用同一套 Trace ID 传递机制（语音 Session 建立时复用/关联 IM Core 的 Trace 上下文）。

## 37. OpenTelemetry 集成边界

- Signaling/控制路径（Token 签发、Room 创建、权限变更）：全链路 Trace，粒度到每次 API 调用。
- Media 层（RTP 包级别）：**不做逐包 Trace**，仅采集周期性聚合统计（bitrate/loss/jitter 等，第36章指标），避免 Observability 本身引入不可忽视的 CPU/网络开销（呼应第37章原始要求）。

## 38. Failure Model

| 故障 | Existing Call | New Call | Reconnect | Text IM | Recovery |
|---|---|---|---|---|---|
| LiveKit Media Node Pod Down | 该节点承载的通话中断 | 无法在该节点建立（路由到其他节点） | 客户端 ICE Restart 尝试重连到新节点 | ✓ 不受影响 | 节点重启/替换后恢复容量，进行中通话需用户重新加入 |
| Media Node 所在 K3s 节点 Down | 同上 | 同上 | 同上 | ✓ 不受影响 | K3s 重调度到其他专用节点（若节点池有冗余） |
| TURN（内置）不可用 | 依赖 TURN 中继的连接可能中断 | 直连尝试仍可能成功，仅 TURN-only 场景受影响 | 重试 ICE，若无 TURN 则仅直连可用网络下可恢复 | ✓ 不受影响 | 参考第39章降级策略 |
| Voice Control Service Down | 进行中通话**不受影响**（已签发 Token 仍有效，LiveKit 独立运作） | ✗ 无法签发新 Token，新语音会话无法建立 | ✗ Token 过期后无法续期，语音会话最终会因 Token 到期而中断 | ✓ 不受影响 | 服务恢复后自动重新可用 |
| Valkey Down（若 Voice Control 依赖其做 Session 状态） | 视具体依赖粒度而定，需避免强依赖 | 可能受影响 | 可能受影响 | ✓ 不受影响 | 建议 Voice Control Service 对 Valkey 采用可降级读（本地短期缓存兜底），避免单点强依赖 |
| K3s 控制面 Node Down | 不影响已运行的 Media Node Pod（K8s 特性：数据面自治） | 视调度能力受影响 | — | ✓ 不受影响 | 常规 K3s HA 恢复流程 |
| Network Partition | 分区内的通话可能维持，跨分区通话中断 | 视分区情况 | — | 视 IM Core 自身分区处理策略 | — |
| Region Down（多区域场景，V2+） | 该区域通话中断 | 路由至其他区域（若已支持，见第31章） | — | ✓（IM Core 多区域策略另行覆盖） | — |

**核心红线（贯穿全文档）**：任何语音相关组件（LiveKit/TURN/Voice Control Service）故障，**文本 IM（消息/好友/Presence/历史）必须保持完全可用**，验证方式为混沌测试：故意关闭 Voice 相关全部组件，回归 SRS IM Core 全部 P0 用例应 100% 通过。

## 39. Graceful Degradation

```
Spatial Voice 计算失败/超时
        ↓ 降级为
Party Voice（退化为固定分组语音，放弃精细空间订阅）
        ↓（若 Party Voice 也不可用）
Text Chat（纯文字，IM Core 独立可用）

AI Voice（NPC/翻译）故障
        ↓ 不影响
Human Voice（正常人类语音通话继续）

TURN 不可用
        ↓
优先尝试常规 ICE 直连路径（而非直接判定语音功能整体不可用）
```

以上降级路径均需在 VOICE-POC-006/009/010（第40章）中实测验证触发条件与实际表现，不能仅作为设计意图停留在文档层面。

## 40. PoC Plan

| PoC | Purpose | Environment | Steps | Metrics | Pass | Fail |
|---|---|---|---|---|---|---|
| VOICE-POC-001 | 2人语音基础可用性 | 单节点K3s | 建连、说话、挂断 | 建连延迟、音频清晰度（主观） | 建连<2s，双向音频正常 | 任一失败 |
| VOICE-POC-002 | 10人Party语音 | 单节点K3s | 10客户端同时加入Party Room | CPU/内存/带宽 | 全员互通无明显延迟 | 出现掉线/明显延迟 |
| VOICE-POC-003 | 100人Room容量 | 压测集群 | 100participant同Room，10人同时说话 | Egress带宽、CPU、丢包率 | 达到第21章模型预测范围内 | 显著偏离模型或崩溃 |
| VOICE-POC-004 | Unity各平台集成 | Windows/macOS/Android/iOS | 逐平台验证麦克风/播放 | 集成耗时、平台兼容清单 | 产出第24章平台表格的确定状态 | 无法在某平台运行且无替代方案 |
| VOICE-POC-005 | Unreal Adapter可行性 | UE5 + Rust Core | 通过C ABI打通收发音频 | 集成耗时、延迟开销 | 可用且延迟增量可接受(需定基线) | Adapter层引入不可接受延迟或无法稳定运行 |
| VOICE-POC-006 | Proximity语音 | 游戏内测试场景 | 模拟玩家移动，验证Audible Set动态调整 | 订阅调整延迟、误听/漏听率 | 调整延迟<候选目标，无明显误听 | 明显延迟或错误听觉范围 |
| VOICE-POC-007 | 丢包场景 | 网络损伤模拟(5-20%丢包) | 通话中人工注入丢包 | 音频可懂度（主观+客观MOS参考） | 20%丢包内仍可理解对话 | 音频不可用 |
| VOICE-POC-008 | 高延迟场景 | 网络损伤模拟(200-400ms RTT) | 同上 | 端到端延迟、可用性 | 达到候选延迟目标 | 明显不可用 |
| VOICE-POC-009 | TURN路径验证 | 强制走TURN(阻断直连) | 强制relay通话 | 建连成功率、延迟增量 | 成功率≥99%，延迟增量在预期内 | 大量失败 |
| VOICE-POC-010 | LiveKit节点故障恢复 | 多节点集群 | 强制下线一个Media Node | 受影响通话数、重连成功率、恢复时间 | 客户端可通过重连恢复到新节点 | 长时间无法恢复 |
| VOICE-POC-011 | 重连（网络切换） | 移动网络模拟Wi-Fi↔4G切换 | 通话中切换网络 | 重连耗时、是否需要完整重新鉴权 | 重连<候选目标时间，无需完整重新鉴权 | 长时间中断或需完整重登录 |
| VOICE-POC-012 | K3s部署验证 | K3s专用节点池 | 按第19章架构实际部署 | hostNetwork生效、UDP端口范围可达 | 外部客户端可成功建连 | 无法从集群外建连 |
| VOICE-POC-013 | HUD+语音联动 | 桌面+游戏内 | 验证说话状态在Laser层正确表达 | 状态更新延迟、是否符合"低干扰"原则 | 符合第28章设计约束 | HUD因语音功能变得干扰性强 |
| VOICE-POC-014 | Push-to-Talk | 键鼠环境 | 验证Hold模式下发布/停止发布Track的及时性 | 按键到音频开始发布的延迟 | 达到候选目标(需先定基线) | 明显滞后或误触发 |
| VOICE-POC-015 | 手柄PTT | 手柄环境 | 验证手柄按键触发PTT且不与游戏输入冲突 | 冲突次数 | 0次冲突 | 出现输入冲突 |

## 41. Benchmark Plan

**VOICE-BENCH-001**：分别测量 100 / 500 / 1K / 5K / 10K+ 并发下的 CPU、Memory、Network、端到端延迟、Packet Loss、Jitter、重连成功率、Publisher/Subscriber 数量对系统的影响。

**重要方法论**：**单 Room 规模测试**（如"一个 100 人 Room 能承受多少同时说话者"）与 **Cluster 整体并发规模测试**（如"整个集群同时存在 10000 个独立的 2 人通话"）必须分开进行，二者压力特征完全不同（前者考验单 Room/单 Node 的 Egress 与 CPU，后者考验 Media Node 池的水平扩展与调度），不得混淆结论。所有具体数字在测试完成前均为 **BENCHMARK REQUIRED**，不得提前写入 SLO 承诺。

## 42. LiveKit vs Alternatives

| Dimension | LiveKit | mediasoup | Janus | Custom SFU |
|---|---|---|---|---|
| License | Apache-2.0，商用友好 | ISC，同样宽松商用友好 | GPLv3（默认），商业闭源需另购商业授权（LEGAL-REVIEW） | 完全自主，但需自行承担全部合规设计 |
| Self-host | ✓ 官方一等公民支持，Helm Chart完善 | ✓ 需自行搭建应用层（仅提供底层库） | ✓ 但通常搭配 SIP 场景，插件生态偏会议 | ✓（自建） |
| Game SDK | Unity官方成熟，Unreal/Godot需自建Adapter | 无面向游戏引擎的官方SDK，纯Node.js/浏览器导向 | 无游戏引擎官方SDK | 需完全自研全部SDK |
| Rust Integration | ✓ 官方`rust-sdks`维护，与本项目Rust技术栈契合度最高 | 无原生Rust SDK（C++核心+Node.js API为主） | 无原生Rust SDK（C核心） | 完全自主 |
| K3s | 有官方Helm Chart，但hostNetwork约束明确（第19章） | 需自行设计K8s部署方案 | 需自行设计 | 完全自主设计 |
| Spatial Voice | 无原生支持，需自建（与其他方案一致） | 无原生支持 | 无原生支持 | 需自建 |
| Scaling | Room/Track模型清晰，社区案例较多 | 需应用层自行实现房间/路由抽象，灵活但工作量大 | 插件化架构，扩展需要C层开发经验 | 完全取决于自研质量 |
| Customization | 中等（高层API，深度定制需改动Go核心） | 高（更底层，编程模型更灵活） | 高（C插件） | 最高，但成本也最高 |
| Operations | 相对成熟（官方文档、Helm、社区案例较多） | 需要更多自建运维工具链 | 运维复杂度较高（历史包袱、模块多） | 全部自建 |
| Development Cost | 低（开箱即用能力最完整） | 中高（更多自研工作） | 中高 | 最高 |
| Commercial Risk | 低（Apache-2.0，无SaaS条款风险） | 低（ISC同样宽松） | 中（GPLv3默认需规避或购买商业授权，LEGAL-REVIEW） | 低许可证风险，但高工程/时间风险 |

**结论**：LiveKit 在"Rust 生态契合度 + 游戏 SDK 起点（Unity）成熟度 + License 商用友好 + 开箱即用运维成熟度"的综合指标上优于 mediasoup 和 Janus，是当前**综合成本最低的起点**；mediasoup 是"若未来需要更深度定制媒体路由逻辑"时的合理候补（同样宽松 License），Janus 因默认 GPLv3 与本项目"全开源可商用无限制"的既定技术栈约束（呼应 SRS 第14a章）存在摩擦，**不推荐**作为主选项。自研 SFU 在当前阶段（MVP/V1）投入产出比最差，不建议。

## 43. ADR Candidates

| ADR | 主题 | Decision Status |
|---|---|---|
| ADR-VOICE-001 | LiveKit Adoption | **Confirmed（结论性采纳，见第48章）**，条件见第45章MVP边界 |
| ADR-VOICE-002 | Voice Control Plane（自建Rust服务，职责范围如第6章） | Confirmed |
| ADR-VOICE-003 | TURN架构（内置 vs 独立coturn） | Candidate：MVP用内置，独立coturn按运营数据触发（Needs PoC/运营数据） |
| ADR-VOICE-004 | K3s Media Deployment（部署形态A~E） | Candidate：推荐A，Needs PoC(VOICE-POC-012) |
| ADR-VOICE-005 | VoiceScope Mapping（尤其Guild三种模型） | Candidate：推荐模型B+C组合，Needs PoC(VOICE-POC-002/003) |
| ADR-VOICE-006 | Spatial Voice Architecture（自建轻量AOI vs 复用游戏AOI） | Candidate：默认复用游戏AOI，无则轻量Grid，Needs PoC(VOICE-POC-006) |
| ADR-VOICE-007 | Game SDK Architecture（Unreal/Godot Adapter方案） | Needs PoC(VOICE-POC-005) |
| ADR-VOICE-008 | Rust Shared Voice Core（仅Unreal/Godot使用，Unity保留官方SDK） | Candidate，见第27章 |
| ADR-VOICE-009 | Multi-region Media | Needs PoC，非MVP范围（第31章原则性预留） |
| ADR-VOICE-010 | Voice Recording（Egress使用策略） | Candidate：默认关闭，按租户显式开启，Needs 法务/合规复核 |

## 44. Risk Register

| Risk ID | 描述 | 概率 | 影响 | 缓解 | PoC | Owner | Status |
|---|---|---|---|---|---|---|---|
| RISK-VOICE-001 | Console SDK兼容性未知，可能需长期自研或平台方审核周期长 | 高 | 高（阻断主机平台上线） | 提前联系平台方/评估WebRTC类中间件在主机的历史先例；不在MVP承诺主机支持 | 无（依赖平台方流程） | SDK团队 | Open |
| RISK-VOICE-002 | 反作弊系统对语音相关网络行为/进程的误判 | 中 | 高 | 语音走标准WebRTC协议+独立进程，避免Hook（复用SRS Overlay风险矩阵原则） | 安全审查 | 安全团队 | Open |
| RISK-VOICE-003 | TURN带宽成本被低估，规模化后成本失控 | 中 | 中 | 持续监控turn_ratio指标，未雨绸缪设计成本告警阈值 | VOICE-POC-009+运营监控 | SRE/成本负责人 | Open |
| RISK-VOICE-004 | SFU Egress带宽在大Room场景被低估导致带宽/成本双重问题 | 中 | 高 | 强制VoiceScope订阅收敛策略（第21章），禁止无脑全互听 | VOICE-POC-003 | 语音架构师 | Open |
| RISK-VOICE-005 | K3s UDP/hostNetwork配置在特定云厂商/网络环境下不可用或配置复杂度超预期 | 中 | 高 | 提前在目标部署环境验证，形成Runbook | VOICE-POC-012 | SRE | Open |
| RISK-VOICE-006 | Media Node故障导致进行中通话中断且恢复不够快 | 中 | 中 | 明确"预期行为"而非承诺无感知迁移，客户端做好重连UX | VOICE-POC-010 | 语音架构师 | Open |
| RISK-VOICE-007 | Proximity场景订阅调整频率过高导致Control Plane或LiveKit API调用压力过大 | 中 | 中 | 增量API+调整频率限流/合并 | VOICE-POC-006 | 语音架构师 | Open |
| RISK-VOICE-008 | 跨区域延迟在未来多区域场景下影响体验 | 低（非MVP范围） | 中 | 架构预留但不在当前阶段投入 | 延后 | 架构师(V2+) | Open |
| RISK-VOICE-009 | 移动网络环境下重连/切换体验差 | 中 | 中 | ICE Restart优化+Token续期机制 | VOICE-POC-011 | 客户端团队 | Open |
| RISK-VOICE-010 | LiveKit版本升级引入Breaking Change，影响自建Adapter(Unreal/Godot)与Voice Control Service | 中 | 中 | 锁定版本+建立升级前回归测试清单，不追新版本 | 常规发布流程 | 语音架构师 | Open |
| RISK-VOICE-011 | Janus/mediasoup对比结论随其版本演进而过时，未来需要重新评估 | 低 | 低 | 本文档结论标注调查时间点，纳入定期技术雷达复审 | 无 | 架构师 | Open |

## 45. MVP（语音子系统范围）

MVP 验证目标：**Party Voice 在 Unity + K3s 上稳定可用，且不影响 IM Core**。

MVP 范围（P0）：
- Party Voice（VoiceScope: Party）+ Push-to-Talk + Hold/Toggle 模式（键鼠）。
- Unity 官方 SDK 集成（Windows 优先，其余平台按 VOICE-POC-004 结果排期）。
- 基础 HUD 集成（Laser 层说话/静音/重连状态）。
- K3s hostNetwork Media Node 部署（选项A）+ LiveKit 内置 TURN。
- Voice Control Service 骨架：Identity Mapping、Token 签发、Party Room 生命周期管理、Server Mute/Kick。
- 基础安全：Token 短生命周期+签名校验、Grants 强制服务端校验。

**MVP 明确不包含**（呼应第45章原始要求，均为 V1/V2/Future）：
- AI Voice / 实时翻译。
- Voice Recording（Egress）。
- 大规模 Guild Voice（子频道模型，第11章）——MVP 仅验证 Party 规模。
- Proximity/空间语音（第13/14章）——技术方案已设计，但实现延后至 V1。
- Multi-region（第19/31章）——架构预留，MVP 单区域。
- Unreal/Godot SDK——MVP 仅 Unity；Unreal Adapter 作为 V1 早期任务（因涉及独立 PoC 与开发周期）。

架构不得为达成 MVP 而堵死上述能力的后续演进（呼应"任何设计冲突优先IM Core可靠性"原则，同时保持扩展空间）。

## 46. Recommendation（见第48章最终判断的展开）

见下方第48章。本章不再重复。

---

# 第47章 自我审查（五轮 Review）

审查发现已直接修订进正文（如 Guild 三模型比较、Unreal SDK 状态如实标注 Requires Custom Port、Janus GPLv3 风险标注等），以下为审查结论摘要：

1. **WebRTC Expert Review**：确认 ICE/TURN/Opus/重连/丢包设计符合标准 WebRTC 实践；强调 Media 层不做逐包 Trace（第37章）避免 Observability 反噬性能；Token 生命周期与 Grants 强制服务端校验作为安全基线写入第8/35章。
2. **Game Network Engineer Review**：确认 Guild "1 Guild=1 Room" 是初稿曾隐含的错误假设，已改为三模型比较并给出组合推荐（第11章）；Match 场景补充明确的 Publish/Subscribe 权限矩阵防偷听（第12章）；Console/手柄状态如实标注为 Unknown/Requires Vendor Approval，不臆测。
3. **K3s/SRE Review**：确认 hostNetwork + 单节点单Pod的硬约束已写入第19章，并明确"不支持完全私有/Serverless集群"的限制，标记为 Open Issue；Media Node 故障恢复被明确为"预期中断+客户端重连"而非无感知迁移，避免过度承诺。
4. **Security Review**：补充 Room Enumeration、TURN Abuse 等初稿遗漏的威胁项（第35章）；强调 Grants 由服务端强制而非客户端自律，是贯穿全文档（Party/Guild/Match/Proximity）的统一安全基线。
5. **Product Review**：删除初稿中"默认采用 Quadtree/Octree 等复杂空间索引"的倾向（第14章明确"不要在MVP/V1阶段自建复杂空间数据库，优先复用游戏AOI"）；将 AI Voice/实时翻译/Multi-region/大规模Guild Voice 全部移出 MVP（第45章）；明确 LiveKit Agents 不采用，避免与 AI Extension 架构重复投入（第31章）。

---

# 第48章 最终判断

1. **LiveKit 是否适合本项目？** 适合，作为 Media Plane 组件，在明确的架构边界约束下采用。
2. **为什么？** Apache-2.0 商用友好、官方 Rust SDK 与本项目技术栈契合、Unity 官方 SDK 成熟、Helm/K3s 生态相对完善、开箱即用能力覆盖 SFU/ICE/TURN/Opus 全部媒体传输基础设施，显著降低自研成本。
3. **最大的5个优势**：① License 无商用限制；② 与 Rust 技术栈原生契合（`rust-sdks`）；③ Unity 官方 SDK 成熟可用；④ 内置 TURN/Egress/Agents 等周边能力齐全，按需启用；⑤ 社区与运维文档相对完善，K3s Helm Chart 现成可用。
4. **最大的5个风险**：① Unreal/Console SDK 不成熟，需自建/等待供应商流程（RISK-VOICE-001）；② hostNetwork 单Pod单节点约束带来的运维复杂度与"不支持私有/Serverless集群"限制（RISK-VOICE-005）；③ 大规模 Room 场景 Egress 带宽/成本失控风险（RISK-VOICE-003/004）；④ Media Node 故障导致进行中通话中断且需依赖重连而非无缝迁移（RISK-VOICE-006）；⑤ 空间语音/Guild社交映射等游戏特有能力完全需要自建，工作量不小（非LiveKit风险，但是采用LiveKit后仍需承担的既定工作量）。
5. **哪些能力LiveKit已经解决？** SFU媒体转发、ICE/STUN/TURN穿透、Opus编解码传输、Room/Track/Subscription基础模型、Token化权限校验、Egress/Ingress基础设施、Unity客户端SDK。
6. **哪些能力必须自己实现？** VoiceScope业务语义映射、Guild/Party/Match/Proximity的具体规则、空间兴趣管理（Audible Set计算）、Voice Control Service（Token签发/权限判定/审计/多租户隔离）、Unreal/Godot Adapter、Game Voice SDK上层封装、HUD集成、AI Voice编排（复用自建AI Extension而非LiveKit Agents）。
7. **哪些能力应该交给Game Server？** 玩家位置数据（Proximity输入）、Party/Guild/Match/Team成员关系的权威状态、加入/离开语音的业务触发事件——Game Server始终是这些游戏世界事实的Authoritative Source，Voice Control Service只是消费者，不重复维护。
8. **LiveKit应该位于什么架构边界？** 严格限定为Media Plane，位于Voice Control Service之下，只处理"已授权的媒体收发"，不接触IM Core数据模型、不参与业务权限判定、不持有游戏社交语义。
9. **MVP是否应该采用LiveKit？** 是，但MVP范围严格收窄为Party Voice + Unity + K3s单区域（第45章），Guild/Proximity/Multi-region/AI Voice等延后。
10. **什么条件出现时应放弃LiveKit，迁移mediasoup或自研？** 当出现以下任一实证信号时应重新评估：① Media Node hostNetwork约束在目标云/部署环境下长期无法达成可接受的运维成本（且社区/官方无改善路线图）；② 深度定制媒体路由的需求（如自定义拥塞控制/自定义转发拓扑）超出LiveKit高层API的可扩展边界，且频繁需要修改LiveKit核心代码维护私有分支；③ LiveKit版本升级持续引入影响自建Adapter（Unreal/Godot）的Breaking Change，维护成本超过自建SFU的边际成本；④ 出现更契合Rust生态且同样商用友好、社区活跃度反超的替代方案。出现①②③④任一项且经ADR复审确认，则启动向mediasoup（License风险最低的候补）迁移评估，而非直接自研。

**最终结论：ADOPT WITH CONDITIONS**（采纳，附带条件：严格的Media Plane边界约束、MVP范围收紧、Unreal/Console/Multi-region等能力的自建工作量需提前纳入路线图与资源规划、License复核事项需法务闭环）。
