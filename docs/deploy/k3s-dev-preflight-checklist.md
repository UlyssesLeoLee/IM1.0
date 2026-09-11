---
doc_id: deploy-k3s-dev-preflight
title_zh: K3s dev 部署前检查清单 (F-2 阻塞解除)
phase: 12-operations
related_wbs: F-1, F-2, F-3, F-4
status: Active
version: 1.0.0
date: 2026-09-01
---

# K3s dev 部署前检查清单 (Pre-flight Checklist)

> 适用范围: **F-2 启动前**(132-wbs §5.6, token 400K-800K, Blocked 等 F-1)
> 配套: `deploy/k3s/dev/` 8 份 manifest + `migrations/0001-0006` SQL
> 验证机制:本清单 5 块,每块可勾选;全部 ☐→☑ 后,`kubectl apply -k deploy/k3s/dev/` 应在 5 分钟内 ready
> 责任方: Mavis (代 Ulysses 排查 F-1, 验证 F-2/F-3/F-4)

## 0. 使用说明 (How to use)

- 每块顶部给出"如果失败"的处理入口
- 每条用 `- [ ]` markdown checkbox,可直接粘到 PR description 当 self-attestation
- 块顺序:**Docker daemon → kubectl → image pull → secret → PVC**(由依赖底层 → 上层)
- 完成本清单后预期: `kubectl get all -n im1-dev` 显示 5 pods Running + 1 Job Complete + 2 Services

---

## 1. Docker (Block 1 / 5)

> **如果失败**:F-1 解锁(详见 132-wbs §5.6 F-1 行)— 当前 `docker ps` 2 分钟超时,WSL2 ↔ Docker Desktop bridge 损坏
> 验收命令: `docker info` 在 5 秒内返回 Client + Server 段

- [ ] `docker --version` 返回 `Docker version 27.x` 或更高
- [ ] `docker info` 5 秒内返回(当前症状:2 分钟超时 → F-1 阻塞)
- [ ] Docker Desktop 状态栏显示 "Engine running"(macOS/Windows)
- [ ] WSL2 集成已启用(Docker Desktop → Settings → Resources → WSL Integration → 勾选 default distro)
- [ ] `wsl -e docker ps` 在 WSL 内能直接调用 docker CLI(无需再启 Docker Desktop)
- [ ] `docker context ls` 当前 context = `desktop-linux`(或自建 k3d context)
- [ ] **修复入口**:若 `docker info` 超时,执行 `wsl --shutdown` → 重启 Docker Desktop → 等 30s 再试

---

## 2. kubectl + K3s cluster (Block 2 / 5)

> **如果失败**:`kubectl cluster-info` 报 "connection refused" → K3s 集群未启动,见 BasicDesign §13
> 验收命令: `kubectl cluster-info` 返回 K3s master URL + DNS 服务

- [ ] `kubectl version --client` 客户端 ≥ 1.30
- [ ] `kubectl version --short` 客户端 / 服务端版本兼容(server 通常是 K3s 自带 1.30+)
- [ ] K3s 服务进程运行中:`systemctl status k3s` (Linux) 或 k3d 容器运行中
- [ ] `kubectl cluster-info` 5 秒内返回 `https://127.0.0.1:6443` + CoreDNS running
- [ ] `kubectl get nodes` 至少 1 个 Ready 节点
- [ ] KUBECONFIG 环境变量已 export(或 `~/.kube/config` 存在 K3s context)
- [ ] 当前 context = K3s(非 Docker Desktop / GKE / EKS):`kubectl config current-context`
- [ ] `kubectl auth can-i create namespace` 返回 `yes`(避免 RBAC 阻断)

---

## 3. image pull (Block 3 / 5)

> **如果失败**:`ImagePullBackOff` → 检查 GHCR 凭据 / image tag 存在 / 网络可达
> 验收命令: `kubectl run test-pull --image=postgres:18.6 --rm -it --restart=Never -- echo OK`

> **本项目使用 3 个镜像**:
> - `postgres:18.6` — DB
> - `valkey/valkey:8` — 缓存
> - `nats:2.10-alpine` — 事件总线
> - `ghcr.io/yourorg/im1.0-im-gateway:latest` — 业务网关(**自建**)
> - `ghcr.io/yourorg/im1.0-im-migrate:latest` — 迁移镜像(**自建**)

- [ ] Docker Hub 可达:`docker pull postgres:18.6` 5 分钟内成功
- [ ] `docker pull valkey/valkey:8` 成功
- [ ] `docker pull nats:2.10-alpine` 成功
- [ ] GHCR 可达:`docker pull ghcr.io/yourorg/im1.0-im-gateway:latest` 成功(自建镜像需先 push)
- [ ] GHCR 可达:`docker pull ghcr.io/yourorg/im1.0-im-migrate:latest` 成功
- [ ] K3s 节点能拉取:`docker save postgres:18.6 | ssh node 'ctr -n k8s.io images import -'`(K3s 节点若用 containerd 而非 docker,可能需手动 import)
- [ ] **如 GHCR 是 private**:K3s 节点有 `imagePullSecrets`(在 default SA 或 im-gateway ServiceAccount 上挂 `regcred`)
- [ ] **CI 路径(后续 F-3 触发)**:`.github/workflows/release.yml` 已 build + push `im-gateway` + `im-migrate` 镜像

---

## 4. Secret 注入 (Block 4 / 5)

> **如果失败**:`CreateContainerConfigError` 缺 secret key / image 无法启动
> 验收命令: `kubectl get secret -n im1-dev` 显示 `postgres-credentials` + `im-core-env` + `im-env-production` 3 个 Secret

> **本项目 10 个 Secret Key**(per ImplementationSpec §1.1):
> - `postgres-credentials` Secret: `username` / `password` / `database`
> - `im-core-env` Secret: `database-url` / `valkey-url` / `nats-url` / `minio-endpoint` / `minio-access-key` / `minio-secret-key` / `jwt-signing-keys` / `refresh-token-pepper`
> - `im-env-production` Secret: `server_secret`(MVP 1 个 env)

- [ ] `kubectl create namespace im1-dev --dry-run=client -o yaml | kubectl apply -f -` 成功
- [ ] **生产用 sealed-secrets**(不要直接 apply `secrets-template.yaml`):`kubeseal --format yaml < secret.yaml > sealed-secret.yaml` 后 commit
- [ ] 或者 dev 用 `kubectl create secret` 直注:`kubectl create secret generic postgres-credentials -n im1-dev --from-literal=username=im --from-literal=password=$(openssl rand -hex 32) --from-literal=database=im1`
- [ ] `im-core-env` 8 个 key 全部注入(JWT 用 `kid` 数组 JSON,长度 ≥ 64 bytes hex)
- [ ] `im-env-production.server_secret` 长度 ≥ 64 bytes hex(供 HMAC-SHA256)
- [ ] `kubectl get secret postgres-credentials -n im1-dev -o jsonpath='{.data.password}' | base64 -d | wc -c` ≥ 32 bytes
- [ ] `secrets-template.yaml` **不要**直接 commit 实际 secret 到 main(必须用 sealed-secrets 或 .gitignore)
- [ ] **可选(V1+)**:external-secrets-operator + AWS Secrets Manager / Vault,本 dev 阶段可省

---

## 5. PVC (Block 5 / 5)

> **如果失败**:`Pending` pod 状态,`kubectl describe pvc` 报 `FailedBinding`
> 验收命令: `kubectl get pvc -n im1-dev` 显示 `postgres-data` Bound

- [ ] K3s 集群有默认 StorageClass:`kubectl get storageclass` 至少 1 个 default(`local-path` K3s 默认)
- [ ] `kubectl get storageclass local-path -o jsonpath='{.provisioner}'` 返回 `rancher.io/local-path`
- [ ] **postgres-data PVC 20Gi**:`kubectl apply -f deploy/k3s/dev/postgres.yaml` 后 30 秒内 Bound
- [ ] 若无 default StorageClass,显式指定:`postgres-data.spec.storageClassName: local-path`
- [ ] **dev 用 emptyDir 也行**(NATS/valkey 当前 manifest 已用 emptyDir,无需 PVC;只有 postgres 需持久化)
- [ ] **节点本地存储检查**:`df -h /var/lib/rancher/k3s/storage` 剩余 ≥ 20GB(postgres-data 20Gi)
- [ ] **备份脚本**(V1+,dev 可省):`scripts/backup-postgres.sh` 用 `pg_dump` → 定时 cron
- [ ] **清理脚本**(重置环境):`kubectl delete pvc postgres-data -n im1-dev` + 重 apply,数据清空

---

## 6. 验收 (Final Verification)

完成 5 块后,执行 `kubectl apply -k deploy/k3s/dev/`,预期:

- [ ] `kubectl get all -n im1-dev`:
  - pods: `postgres-xxx` Running + `valkey-xxx` Running + `nats-xxx` Running + `im-gateway-xxx` Running
  - job: `im-migrate-xxx` Complete(0/1)
  - services: `postgres` + `valkey` + `nats` + `im-gateway` ClusterIP
  - ingress: `im-gateway` Address 分配
- [ ] `kubectl get pvc -n im1-dev`:`postgres-data` Bound
- [ ] `kubectl logs -n im1-dev im-migrate-xxx` 显示 6 个 migration applied(0001-0006)
- [ ] `kubectl exec -n im1-dev deploy/postgres -- psql -U im -d im1 -c '\dt'` 列出 14 张表
- [ ] `curl http://im-dev.example.com/healthz` 返回 200(需先配 /etc/hosts 或 ingress 解析)
- [ ] `curl http://im-dev.example.com/readyz` 返回 200(检查 postgres + valkey + nats 三方连接)
- [ ] **WBS 验收**:F-2 状态 Todo → WIP → Done,F-3 / F-4 解锁

## 7. 已知缺口 (Known Gaps)

- 本清单不包含**镜像构建**步骤(假设 `release.yml` 已 build + push);若自建 dev 镜像,需先 `docker build -f docker/im-gateway.Dockerfile -t ghcr.io/yourorg/im1.0-im-gateway:latest .`
- 不包含**TLS 证书**(dev 用 HTTP,cert-manager + Let's Encrypt 在 V1 staging/prod 配)
- 不包含**Prometheus / Grafana**(F-4 之后补,本清单只到 F-2 跑通)
- **ingress 当前 manifest 用 nginx**(per 132-wbs §5.6 + per aux-13 历史);Mavis 接手 2026-09-01 13:03 JST 后**所有 nginx 应替换为 envoy**,但本清单不动 ingress(等 F-2 解锁后的独立 task)

## 8. 关联文档 (References)

- 上游: 132-wbs.md §5.6 F-1 / F-2 / F-3 / F-4
- 上游: ImplementationSpec.md §8 K3s 部署 + §13 部署清单
- 上游: BasicDesign.md §13 K3s 部署架构
- 上游: BasicDesign.md §14.4 Secret 管理
- 配套: deploy/k3s/dev/kustomization.yaml(8 manifests 总入口)
- 配套: docker/im-gateway.Dockerfile + docker/im-core.Dockerfile
- 配套: .github/workflows/deploy-dev.yml(F-3 触发)

## 9. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | 2026-09-01 | 架构师 (Mavis 接手 agent per DEC-008) | 初版:5 块 (Docker / kubectl / image / secret / PVC) + 验收 7 条,每条 `- [ ]` 可勾选,补 132-wbs F-2 解锁前置清单缺口 |
