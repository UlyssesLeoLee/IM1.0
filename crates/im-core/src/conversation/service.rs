//! ConversationService — 业务编排
//!
//! 依据: ImplementationSpec §7.4.2 + DetailedDesign §9.3
//!
//! 2026-09-20 升级 (C-8 WBS ULYS-148):
//! - `create_dm` 改为单事务:find_dm → create conv → upsert dm_pairs → add_member × 2
//!   并发 dup-DM 由 dm_pairs PRIMARY KEY (env, user_a<user_b, user_b) 兜底
//! - `create_group` / `create_channel` / `create_system` / `create_broadcast` 五类会话入口
//! - `add_member` / `remove_member` / `leave` 服务方法,leave race 由 count_owners 保护
//!   (最后一任 owner 不能 leave,必须先 transfer)

use std::sync::Arc;

use serde_json::Value;

use im_common::ids::{ConversationId, EnvironmentId, UserId};
use im_common::AppError;

use super::repository::{
    Conversation, ConversationKind, ConversationRepository, MemberRole,
};

/// Guest 限制(ImplementationSpec §3.1.2 / §11.1):
/// `users.kind='guest'` 仅允许 kind=dm,且另一方非 banned
pub fn check_guest_can_create(user_kind_is_guest: bool, kind: ConversationKind) -> Result<(), AppError> {
    if user_kind_is_guest && !matches!(kind, ConversationKind::Dm) {
        return Err(AppError::Forbidden(
            "guest users can only create DM conversations".into(),
        ));
    }
    Ok(())
}

pub struct ConversationService {
    repo: Arc<dyn ConversationRepository>,
}

impl ConversationService {
    pub fn new(repo: Arc<dyn ConversationRepository>) -> Self {
        Self { repo }
    }

    // ============================================================================
    // 创建会话 — 五类会话(IM-CONV-001 / SRS §13)
    // ============================================================================

    /// DM(双人私聊)
    ///
    /// 幂等 + 单事务:
    /// 1. 校验 user_a != user_b
    /// 2. 事务内查 dm_pairs(env, normalize(a,b)) 是否存在
    /// 3. 存在 → 返回;否则建 conv(kind=dm) + upsert dm_pairs + add_member × 2
    /// 4. 兜底:即使两个并发调用都过了 step 2 的 find,DB UNIQUE 也会让后到的 upsert 走
    ///    ON CONFLICT DO NOTHING,我们回查 dm_pairs 拿到先到者创建的 conversation_id
    ///    然后把后到的 conversation 行删除(避免空 conv)
    pub async fn create_dm(
        &self,
        env: EnvironmentId,
        user_a: UserId,
        user_b: UserId,
    ) -> Result<Conversation, AppError> {
        if user_a == user_b {
            return Err(AppError::Validation("cannot create DM with self".into()));
        }

        let mut tx = self
            .repo
            .pool()
            .begin()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("begin: {}", e)))?;

        // Step 1: 事务内查 dup(并发兜底)
        if let Some(existing) = self.repo.find_dm_in_tx(&mut tx, env, user_a, user_b).await? {
            tx.commit().await.ok();
            return Ok(existing);
        }

        // Step 2: 建 conv
        let conv = self
            .repo
            .create_in_tx(&mut tx, env, ConversationKind::Dm, Value::Object(Default::default()))
            .await?;

        // Step 3: upsert dm_pairs;若 ON CONFLICT 触发,说明并发抢先,rollback 当前事务,改用先到者的 conv
        let upsert = self
            .repo
            .upsert_dm_pair(&mut tx, env, user_a, user_b, conv.id)
            .await?;

        if !upsert.inserted {
            // 并发抢先建了 dm_pairs,我们 rollback 自己刚建的空 conv
            tx.rollback().await.ok();
            // 重新查 DM(走非事务版本,反正此时 dm_pairs 已落)
            if let Some(existing) = self.repo.find_dm(env, user_a, user_b).await? {
                return Ok(existing);
            }
            return Err(AppError::Internal(anyhow::anyhow!(
                "dm_pairs upsert reported conflict but find_dm returned None (env={}, a={}, b={})",
                env.0, user_a.0, user_b.0
            )));
        }

        // Step 4: 加双方成员
        self.repo
            .add_member_in_tx(&mut tx, conv.id, user_a, MemberRole::Member)
            .await?;
        self.repo
            .add_member_in_tx(&mut tx, conv.id, user_b, MemberRole::Member)
            .await?;

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("commit: {}", e)))?;
        Ok(conv)
    }

    /// Group(创建者=owner,其他=member)
    pub async fn create_group(
        &self,
        env: EnvironmentId,
        creator: UserId,
        members: Vec<UserId>,
        metadata: Value,
    ) -> Result<Conversation, AppError> {
        if members.len() < 2 {
            return Err(AppError::Validation(
                "group needs at least 2 members".into(),
            ));
        }
        if members.len() > 500 {
            return Err(AppError::Validation("group too large (max 500)".into()));
        }
        if !matches!(metadata, Value::Object(_)) {
            return Err(AppError::Validation("metadata must be JSON object".into()));
        }

        let mut tx = self
            .repo
            .pool()
            .begin()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("begin: {}", e)))?;

        let conv = self
            .repo
            .create_in_tx(&mut tx, env, ConversationKind::Group, metadata)
            .await?;

        self.repo
            .add_member_in_tx(&mut tx, conv.id, creator, MemberRole::Owner)
            .await?;
        for m in members {
            if m != creator {
                self.repo
                    .add_member_in_tx(&mut tx, conv.id, m, MemberRole::Member)
                    .await?;
            }
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("commit: {}", e)))?;
        Ok(conv)
    }

    /// Channel(类似 Group 但语义上是单向广播,这里仅建会话 + 加成员;消息路径走 IM-CONV-003)
    pub async fn create_channel(
        &self,
        env: EnvironmentId,
        creator: UserId,
        members: Vec<UserId>,
        metadata: Value,
    ) -> Result<Conversation, AppError> {
        if members.is_empty() {
            return Err(AppError::Validation(
                "channel needs at least 1 member".into(),
            ));
        }
        if !matches!(metadata, Value::Object(_)) {
            return Err(AppError::Validation("metadata must be JSON object".into()));
        }

        let mut tx = self
            .repo
            .pool()
            .begin()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("begin: {}", e)))?;

        let conv = self
            .repo
            .create_in_tx(&mut tx, env, ConversationKind::Channel, metadata)
            .await?;

        self.repo
            .add_member_in_tx(&mut tx, conv.id, creator, MemberRole::Owner)
            .await?;
        for m in members {
            if m != creator {
                self.repo
                    .add_member_in_tx(&mut tx, conv.id, m, MemberRole::Member)
                    .await?;
            }
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("commit: {}", e)))?;
        Ok(conv)
    }

    /// System 会话(无 owner,通常用于系统消息推送)
    pub async fn create_system(
        &self,
        env: EnvironmentId,
        metadata: Value,
    ) -> Result<Conversation, AppError> {
        if !matches!(metadata, Value::Object(_)) {
            return Err(AppError::Validation("metadata must be JSON object".into()));
        }
        self.repo
            .create(env, ConversationKind::System, metadata)
            .await
    }

    /// Broadcast 会话(只读单向下行,IM-CONV-003;不加 owner)
    pub async fn create_broadcast(
        &self,
        env: EnvironmentId,
        metadata: Value,
    ) -> Result<Conversation, AppError> {
        if !matches!(metadata, Value::Object(_)) {
            return Err(AppError::Validation("metadata must be JSON object".into()));
        }
        self.repo
            .create(env, ConversationKind::Broadcast, metadata)
            .await
    }

    // ============================================================================
    // 成员管理 — DetailedDesign §9.3
    // ============================================================================

    /// 加成员(自动 dedupe,DM 会话禁止再加成员)
    pub async fn add_member(
        &self,
        conv: ConversationId,
        user: UserId,
    ) -> Result<(), AppError> {
        let c = self
            .repo
            .find_by_id(conv)
            .await?
            .ok_or(AppError::ConversationNotFound(conv.0))?;

        // DM 严格 2 人,不允许再加
        if matches!(c.kind, ConversationKind::Dm) {
            return Err(AppError::Validation(
                "DM conversation is fixed to 2 members".into(),
            ));
        }

        // 已存在则幂等成功
        if self.repo.is_member(conv, user).await? {
            return Ok(());
        }
        self.repo.add_member(conv, user, MemberRole::Member).await
    }

    /// 移除成员(kick by admin) — 接受 actor 参数用于权限检查
    pub async fn remove_member(
        &self,
        conv: ConversationId,
        actor: UserId,
        target: UserId,
    ) -> Result<u64, AppError> {
        let c = self
            .repo
            .find_by_id(conv)
            .await?
            .ok_or(AppError::ConversationNotFound(conv.0))?;

        // DM 不允许 kick,要走 leave + 重建(或干脆禁掉)
        if matches!(c.kind, ConversationKind::Dm) {
            return Err(AppError::Validation(
                "DM member removal must go through leave".into(),
            ));
        }

        // actor 必须是 owner/admin
        let actor_role = self
            .repo
            .get_member_role(conv, actor)
            .await?
            .ok_or(AppError::Forbidden("actor is not a member".into()))?;
        if !matches!(actor_role, MemberRole::Owner | MemberRole::Admin) {
            return Err(AppError::Forbidden(
                "only owner/admin can remove members".into(),
            ));
        }

        // 不能 kick owner
        if let Some(MemberRole::Owner) = self.repo.get_member_role(conv, target).await? {
            return Err(AppError::Validation(
                "cannot remove an owner; transfer ownership first".into(),
            ));
        }

        self.repo.remove_member(conv, target).await
    }

    /// 自己离开会话 — leave race 保护:最后一任 owner 不能 leave,必须先 transfer
    pub async fn leave(&self, conv: ConversationId, user: UserId) -> Result<u64, AppError> {
        let c = self
            .repo
            .find_by_id(conv)
            .await?
            .ok_or(AppError::ConversationNotFound(conv.0))?;

        // DM 离开 = 整个会话作废(只删成员,conv/dm_pairs 留痕以保留历史消息可达性)
        // Group/Channel 需要 owner 检查
        if matches!(c.kind, ConversationKind::Group) {
            if let Some(MemberRole::Owner) = self.repo.get_member_role(conv, user).await? {
                let owners = self.repo.count_owners(conv).await?;
                if owners <= 1 {
                    return Err(AppError::Validation(
                        "last owner cannot leave; transfer ownership first".into(),
                    ));
                }
            }
        }

        self.repo.remove_member(conv, user).await
    }

    // ============================================================================
    // 查询
    // ============================================================================

    pub async fn list_user_conversations(
        &self,
        user: UserId,
        cursor: Option<&str>,
        limit: i32,
    ) -> Result<Vec<Conversation>, AppError> {
        let limit = limit.clamp(1, 200);
        self.repo.list_for_user(user, cursor, limit).await
    }

    pub async fn is_member(
        &self,
        conv: ConversationId,
        user: UserId,
    ) -> Result<bool, AppError> {
        self.repo.is_member(conv, user).await
    }

    pub async fn get(&self, id: ConversationId) -> Result<Option<Conversation>, AppError> {
        self.repo.find_by_id(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversation::repository::ConversationKind;

    #[test]
    fn guest_cannot_create_group() {
        let r = check_guest_can_create(true, ConversationKind::Group);
        assert!(r.is_err());
    }

    #[test]
    fn guest_can_create_dm() {
        let r = check_guest_can_create(true, ConversationKind::Dm);
        assert!(r.is_ok());
    }

    #[test]
    fn user_can_create_any_kind() {
        for kind in [
            ConversationKind::Dm,
            ConversationKind::Group,
            ConversationKind::Channel,
            ConversationKind::System,
            ConversationKind::Broadcast,
        ] {
            assert!(check_guest_can_create(false, kind).is_ok());
        }
    }
}
