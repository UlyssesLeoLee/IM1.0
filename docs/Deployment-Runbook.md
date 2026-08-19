# K3s 部署与运维手册（MVP）

版本：v0.1 Draft ｜ 面向：负责部署与运维本 IM 平台的 SRE / 平台工程师
上游依据：`docs/BasicDesign.md`（第13章部署清单）、`docs/DetailedDesign.md`（配置项）、`docs/SRS.md`（第38/39/41章可用性与灾难恢复要求）

本手册只覆盖 **MVP 单区域部署**；多区域、语音子系统（LiveKit Media Node 的 hostNetwork 专用节点池）见 `docs/LiveKit-Voice-Subsystem.md` 第19章，本手册不重复。

---

## 1. 前置条件

- 一个可用的 K3s 集群（单节点用于开发/测试，3节点以上用于生产，呼应 SRS `OPS-NFR-001`）。
- `kubectl`、`helm` 已配置并指向目标集群。
- 已规划好的域名与 TLS 证书（建议 cert-manager + Let's Encrypt，二者均为开源可商用组件，符合 SRS 技术栈约束）。

## 2. 命名空间与依赖组件安装顺序

```bash
kubectl create namespace im-platform

# 1. PostgreSQL（CloudNativePG Operator，见 BasicDesign §13 ADR-012）
helm repo add cnpg https://cloudnative-pg.github.io/charts
helm install cnpg-operator cnpg/cloudnative-pg -n cnpg-system --create-namespace
kubectl apply -n im-platform -f deploy/k3s/postgres-cluster.yaml

# 2. Valkey
helm install valkey oci://registry-1.docker.io/bitnamicharts/valkey -n im-platform

# 3. NATS (JetStream enabled)
helm repo add nats https://nats-io.github.io/k8s/helm/charts/
helm install nats nats/nats -n im-platform --set config.jetstream.enabled=true

# 4. MinIO
helm repo add minio https://charts.min.io/
helm install minio minio/minio -n im-platform

# 5. 应用服务（im-gateway / im-core / im-presence / im-media / extension-runtime / web-dashboard）
helm install im-platform deploy/k3s/im-platform-chart -n im-platform -f deploy/k3s/values-production.yaml
```

**顺序原则**：有状态依赖（PG/Valkey/NATS/MinIO）先于无状态应用服务安装，避免应用启动时因依赖不可用而 CrashLoopBackOff（虽然应用应具备重试机制，但避免不必要的启动噪音）。

## 3. 数据库迁移执行

```bash
# 迁移文件见 migrations/（详见 DetailedDesign.md §8）
kubectl run im-migrate --rm -it --restart=Never \
  --image=<your-registry>/im-core:latest \
  --env="IM_DATABASE_URL=$(kubectl get secret im-db-credentials -o jsonpath='{.data.url}' | base64 -d)" \
  -- sqlx migrate run
```

生产环境建议将迁移步骤纳入 CI/CD 流水线的部署前置 Job，而非手工执行；本手册给出的是最小可运行命令，流水线化留给实施阶段的 CI 配置。

## 4. 首次租户/环境初始化（MVP：手工/脚本方式，暂无自助控制台）

```sql
INSERT INTO tenants (id, name) VALUES (gen_random_uuid(), 'My Studio');
INSERT INTO games (id, tenant_id, name) VALUES (gen_random_uuid(), '<tenant_id>', 'My Game');
INSERT INTO environments (id, game_id, name) VALUES (gen_random_uuid(), '<game_id>', 'production');
```

对应的 `server_secret` / `jwt_signing_key` 需在此时生成并存入 K3s Secret（见第6章），MVP 无自助 Dashboard 生成流程（Dashboard 的租户自助管理为 V1+ 能力）。

## 5. 健康检查与就绪探针配置约定

| 服务 | Liveness | Readiness |
|---|---|---|
| im-gateway | `GET /healthz`（进程存活） | `GET /healthz`（额外检查到 im-core 的 gRPC 连接是否建立） |
| im-core | `GET /healthz` | 额外检查 PostgreSQL 连接池可用 |
| im-presence | `GET /healthz` | 额外检查 Valkey 连接 |
| im-media | `GET /healthz` | 额外检查 MinIO 连接 |

探针失败阈值/间隔留给 Helm Chart `values.yaml` 配置，建议初始值：`initialDelaySeconds=5, periodSeconds=10, failureThreshold=3`（**Candidate**，需按实际启动耗时调整）。

## 6. 密钥管理操作

```bash
kubectl create secret generic im-env-<environment_id> \
  --from-literal=server_secret="$(openssl rand -hex 32)" \
  --from-literal=jwt_signing_key="$(openssl rand -hex 32)" \
  -n im-platform
```

**轮换流程**（呼应 SRS `SEC-NFR-003` Token Rotation 要求，与 LiveKit 文档 `ISS-004` 同类问题的落地）：
1. 生成新密钥，写入新 Secret Key（不覆盖旧的，如 `server_secret_v2`）。
2. 应用配置改为同时接受 `v1`/`v2` 双密钥校验（需应用层支持多密钥校验窗口，实现时需在 `im-core` 的 Token 校验逻辑中预留此能力——**本条为详细设计阶段的遗留实现要求，非纯运维步骤**）。
3. 确认所有客户端/游戏服务器已切换到 v2 签发的 Token 后，下线 v1。

MVP 阶段若尚未实现双密钥校验窗口，轮换将导致存量 Token 立即失效（可接受，因 Access Token TTL 候选值仅15分钟，短暂重新登录影响可控），Refresh Token 层面的轮换影响需提前公告维护窗口。

## 7. 常见故障排查（Runbook 条目）

### 7.1 im-gateway Pod 持续重启

```bash
kubectl logs -n im-platform deploy/im-gateway --previous
kubectl describe pod -n im-platform -l app=im-gateway
```
常见原因：无法连接 im-core（检查 Service DNS/NetworkPolicy）、Valkey 连接串错误、启动时 gRPC 握手超时（检查 im-core 是否先于 gateway 就绪）。

### 7.2 消息发送延迟升高

```bash
# 查看 Grafana 面板（假设已接入 OTel，见 BasicDesign §15）中的 message_latency
# 排查顺序：Client → Gateway → Core → Persistence/Event → Fanout → Client（呼应 SRS 第37章）
kubectl top pods -n im-platform    # 快速判断是否CPU/内存瓶颈
```
若 `message_latency` 高但 `active_connections` 正常，优先怀疑 PostgreSQL 写入延迟（`pg_stat_activity` 检查慢查询/锁等待，参考 DetailedDesign §5 Sequence生成的行锁设计，高并发单会话写入是已知的潜在瓶颈点）。

### 7.3 PostgreSQL 主库故障切换

CloudNativePG Operator 自动处理 Failover（Candidate行为，需按 SRS `VOICE-POC-009` 同类方法论在本环境实测验证，本手册不假设"零配置即可用"）。切换期间 `im-core` 应因连接池重试机制自动恢复，若超过 60s 未恢复，检查：
```bash
kubectl get cluster -n im-platform   # CloudNativePG CRD 状态
kubectl logs -n im-platform -l cnpg.io/cluster=im-postgres
```

### 7.4 NATS JetStream 消息积压（queue_depth 告警）

```bash
kubectl exec -n im-platform nats-box -- nats stream info im-events
```
排查 Consumer 是否卡住（常见于 extension-runtime 在 MVP 骨架阶段若被误接入了阻塞逻辑）；确认无消费者卡死后可安全增加 Consumer 并发度。

## 8. 扩缩容指引

| 服务 | 扩容触发信号 | 操作 |
|---|---|---|
| im-gateway | `active_connections` 接近单Pod承载上限（**BENCHMARK REQUIRED** 确定具体阈值，见SRS POC-01方法论） | `kubectl scale deploy/im-gateway --replicas=N` 或配置HPA |
| im-core | CPU持续>70%，或`message_latency` P95上升 | 同上；注意im-core为无状态，可安全水平扩展，瓶颈通常先出现在PostgreSQL | 
| PostgreSQL | 连接数/QPS接近容量上限 | 优先垂直扩容（更大规格节点），水平扩展（读副本）为V1+按需评估，MVP不预设 |

## 9. 灾难恢复（DR）演练清单（对应 SRS 第41章，MVP最小集）

- [ ] 每周验证一次 PostgreSQL 备份可恢复性（恢复到隔离环境并校验数据完整性，而非仅确认备份文件存在）。
- [ ] 每季度执行一次 K3s 节点故障演练（参考 SRS `POC-08` 方法论：强制下线一个节点，验证 Pod 重调度与服务恢复）。
- [ ] MinIO 多副本/纠删码配置定期检查（`mc admin info`）。

RPO/RTO 具体数值为 Candidate，需在完成第一轮 DR 演练后回填到 SRS SLO 候选表（`docs/SRS.md` 第49章），本手册不预先承诺未经验证的数字。
