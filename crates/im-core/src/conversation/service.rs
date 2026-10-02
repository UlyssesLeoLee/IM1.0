//! ConversationService — 业务编排
//!
//! 依据: ImplementationSpec §7.4.2 + DetailedDesign §9.3

use std::sync::Arc;

use serde_json::Value;

use im_common::ids::{ConversationId, EnvironmentId, UserId};
use im_common::AppError;

use super::repository::{Conversation, ConversationKind, ConversationRepository, MemberRole};

/// Guest 限制(ImplementationSpec §3.1.2 / §11.1):
/// `users.kind='guest'` 仅允许 kind=dm,且另一方非 banned
pub fn check_guest_can_create(
    user_kind_is_guest: bool,
    kind: ConversationKind,
) -> Result<(), AppError> {
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

    /// 暴露底层 repo(用于跨 service 显式调用,如 C-10 list_members)
    /// MVP Day 3:留给 im-gateway 直接 list_members 用,C-9 之后考虑把 list_members
    /// 提升到 service 层(避免泄漏 repo)
    pub fn repo(&self) -> &Arc<dyn ConversationRepository> {
        &self.repo
    }

    pub async fn create_dm(
        &self,
        env: EnvironmentId,
        user_a: UserId,
        user_b: UserId,
    ) -> Result<Conversation, AppError> {
        if user_a == user_b {
            return Err(AppError::Validation("cannot create DM with self".into()));
        }

        // 幂等:已存在返回原会话
        if let Some(existing) = self.repo.find_dm(env, user_a, user_b).await? {
            return Ok(existing);
        }

        // user_a < user_b 规范化(dm_pairs CHECK 约束)
        let (a, b) = if user_a.0 < user_b.0 {
            (user_a, user_b)
        } else {
            (user_b, user_a)
        };

        let conv = self
            .repo
            .create(env, ConversationKind::Dm, Value::Object(Default::default()))
            .await?;

        self.repo.add_member(conv.id, a, MemberRole::Member).await?;
        self.repo.add_member(conv.id, b, MemberRole::Member).await?;
        Ok(conv)
    }

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

        let conv = self
            .repo
            .create(env, ConversationKind::Group, metadata)
            .await?;

        self.repo
            .add_member(conv.id, creator, MemberRole::Owner)
            .await?;
        for m in members {
            if m != creator {
                self.repo.add_member(conv.id, m, MemberRole::Member).await?;
            }
        }
        Ok(conv)
    }

    pub async fn list_user_conversations(
        &self,
        user: UserId,
        cursor: Option<&str>,
        limit: i32,
    ) -> Result<Vec<Conversation>, AppError> {
        let limit = limit.clamp(1, 200);
        self.repo
            .list_for_user(user, cursor, limit.clamp(1, 50))
            .await
    }

    /// 用户所属的**全部**会话 id(无上限)—— WS 广播成员过滤用
    ///
    /// 与 `list_user_conversations` 分开是有意的, 不是重复: 后者给 UI 列表
    /// (完整行 + 排序 + 上限 50), 本方法给广播过滤(只要 id, **一个都不能少**)。
    /// 复用的后果是「加入 >50 个会话的用户收不到老会话的实时消息」且无任何报错。
    /// 详见 `ConversationRepository::list_all_memberships_for_user` 的说明。
    pub async fn list_membership_ids(&self, user: UserId) -> Result<Vec<ConversationId>, AppError> {
        self.repo.list_all_memberships_for_user(user).await
    }

    pub async fn is_member(&self, conv: ConversationId, user: UserId) -> Result<bool, AppError> {
        self.repo.is_member(conv, user).await
    }

    /// 上报已读 (per aux-04 §B.4 转换表 line 239: `delivered` → `read`)
    ///
    /// guard: **会话成员**(转换表写「receiver 在线且为会话成员」)。非成员返
    /// `Forbidden` —— 不能让任意用户给任意会话写 `last_read_sequence`, 那等于
    /// 让他影响别人的未读数。
    ///
    /// 返回 `Ok(true)` = 读指针真的推进了; `Ok(false)` = 已在该 sequence 或更靠后
    /// (**幂等重放, 正常**)。调用方据此决定要不要 fanout —— 对未推进的重复上报
    /// 再广播一次已读事件是纯浪费。
    ///
    /// ## 为什么 fanout 不在本方法里
    ///
    /// 转换表的 effect 写的是「UPDATE last_read_sequence **+ fanout**」, 但
    /// `ServerFrame` 的 10 个变体里**没有已读回执帧**(read receipt)。往哪个帧
    /// 上捎带都是擅自发明 wire 形状, 与 §1.6 的 `UNSUPPORTED_OPERATION`、
    /// §1.8.2 的 `auth_ok` 同一类错误。故此处只做可确证的 UPDATE 部分,
    /// fanout 缺口记在 `docs/gap-ledger.md` §1.12。
    pub async fn mark_read(
        &self,
        conv: ConversationId,
        user: UserId,
        sequence: i64,
    ) -> Result<bool, AppError> {
        if !self.repo.is_member(conv, user).await? {
            return Err(AppError::Forbidden("not a conversation member".into()));
        }
        self.repo
            .advance_last_read_sequence(conv, user, sequence)
            .await
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
        ] {
            assert!(check_guest_can_create(false, kind).is_ok());
        }
    }
}
