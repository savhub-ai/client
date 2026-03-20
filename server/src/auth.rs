use diesel::prelude::*;
use salvo::Request;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::{UserRow, UserTokenRow};
use crate::schema::{user_tokens, users};
use crate::state::app_state;
use shared::{UserRole, UserSummary};

#[derive(Debug, Clone)]
pub struct RequestUser {
    pub id: Uuid,
    pub handle: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: UserRole,
}

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub user: RequestUser,
    pub token_name: String,
}

impl RequestUser {
    pub fn summary(&self) -> UserSummary {
        UserSummary {
            id: self.id,
            handle: self.handle.clone(),
            display_name: self.display_name.clone(),
            avatar_url: self.avatar_url.clone(),
            role: self.role,
        }
    }
}

pub fn parse_role(value: &str) -> UserRole {
    match value {
        "admin" => UserRole::Admin,
        "moderator" => UserRole::Moderator,
        _ => UserRole::User,
    }
}

pub fn optional_auth(req: &Request) -> Result<Option<AuthContext>, AppError> {
    let Some(header_value) = req.headers().get("authorization") else {
        return Ok(None);
    };
    let header_value = header_value
        .to_str()
        .map_err(|_| AppError::Unauthorized("invalid authorization header".to_string()))?;
    let token = header_value
        .strip_prefix("Bearer ")
        .or_else(|| header_value.strip_prefix("bearer "))
        .ok_or_else(|| AppError::Unauthorized("expected a bearer token".to_string()))?;

    let state = app_state();
    let mut conn = state
        .pool
        .get()
        .map_err(|error| AppError::Internal(error.to_string()))?;

    let token_row = user_tokens::table
        .filter(user_tokens::token.eq(token))
        .select(UserTokenRow::as_select())
        .first(&mut conn)
        .optional()?
        .ok_or_else(|| AppError::Unauthorized("token not recognized".to_string()))?;

    let user_row = users::table
        .find(token_row.user_id)
        .select(UserRow::as_select())
        .first(&mut conn)?;

    Ok(Some(AuthContext {
        user: RequestUser {
            id: user_row.id,
            handle: user_row.handle,
            display_name: user_row.display_name,
            avatar_url: user_row.avatar_url,
            role: parse_role(&user_row.role),
        },
        token_name: token_row.name,
    }))
}

pub fn require_auth(req: &Request) -> Result<AuthContext, AppError> {
    optional_auth(req)?.ok_or_else(|| AppError::Unauthorized("authentication required".to_string()))
}

pub fn require_staff(auth: &AuthContext) -> Result<(), AppError> {
    match auth.user.role {
        UserRole::Admin | UserRole::Moderator => Ok(()),
        UserRole::User => Err(AppError::Forbidden(
            "moderator or admin access required".to_string(),
        )),
    }
}

pub fn require_admin(auth: &AuthContext) -> Result<(), AppError> {
    match auth.user.role {
        UserRole::Admin => Ok(()),
        UserRole::Moderator | UserRole::User => {
            Err(AppError::Forbidden("admin access required".to_string()))
        }
    }
}

pub fn can_manage_owner(auth: &AuthContext, owner_user_id: Uuid) -> bool {
    auth.user.id == owner_user_id || matches!(auth.user.role, UserRole::Admin | UserRole::Moderator)
}
