# IM1.0

IM 通信软件，适合 AI 和工作场景，便于集成进游戏的通信软件。

以 IM 即时通信为核心，游戏为首要垂直场景，向 AI 与工作场景自然扩展的可嵌入式实时通信平台。技术栈以 Rust 为主、Python 为辅（限 AI 扩展场景），前端使用 Next.js，部署基于 K3s，全部依赖开源且可商用。

## 文档

- [软件需求规格说明书（SRS）](docs/SRS.md) —— 产品定位、IM Core 与扩展架构、游戏身份桥接、Laser HUD 交互模型、安全与合规、MVP 与路线图、PoC 计划、ADR 候选、风险登记表。
- [LiveKit 实时语音子系统技术调查与需求定义书](docs/LiveKit-Voice-Subsystem.md) —— LiveKit 架构/License 调查、Voice Control Plane 设计、VoiceScope（Party/Guild/Match/Proximity）、K3s 部署约束、安全威胁建模、PoC 与基准测试计划、最终采纳结论。
- [基本设计书（MVP）](docs/BasicDesign.md) —— 服务拓扑与拆分依据、数据模型、消息 Sequence/幂等设计、Identity/Token API、事件总线 Topic、Game SDK 接入映射、Laser HUD 方案、K3s 部署清单。
