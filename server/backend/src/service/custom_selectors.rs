use chrono::Utc;
use diesel::prelude::*;
use serde_json::{json, Value};
use shared::{CustomSelectorsResponse, SaveCustomSelectorsRequest};
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::error::AppError;
use crate::models::{NewUserCustomSelectorsRow, UserCustomSelectorsRow};
use crate::schema::user_custom_selectors;
use crate::service::helpers::{db_conn, insert_audit_log};

/// Return the authenticated user's custom selectors.
pub fn get_custom_selectors(auth: &AuthContext) -> Result<CustomSelectorsResponse, AppError> {
    let mut conn = db_conn()?;
    let row = user_custom_selectors::table
        .filter(user_custom_selectors::user_id.eq(auth.user.id))
        .select(UserCustomSelectorsRow::as_select())
        .first::<UserCustomSelectorsRow>(&mut conn)
        .optional()?;

    match row {
        Some(row) => {
            let selectors = row
                .selectors
                .as_array()
                .cloned()
                .unwrap_or_default();
            Ok(CustomSelectorsResponse {
                ok: true,
                selectors,
                version: row.version as u8,
                updated_at: Some(row.updated_at),
            })
        }
        None => Ok(CustomSelectorsResponse {
            ok: true,
            selectors: Vec::new(),
            version: 1,
            updated_at: None,
        }),
    }
}

/// Upsert the authenticated user's custom selectors.
pub fn save_custom_selectors(
    auth: &AuthContext,
    request: SaveCustomSelectorsRequest,
) -> Result<CustomSelectorsResponse, AppError> {
    let mut conn = db_conn()?;
    let now = Utc::now();
    let selectors_json = Value::Array(request.selectors.clone());

    let existing = user_custom_selectors::table
        .filter(user_custom_selectors::user_id.eq(auth.user.id))
        .select(UserCustomSelectorsRow::as_select())
        .first::<UserCustomSelectorsRow>(&mut conn)
        .optional()?;

    let row_id = if let Some(existing) = existing {
        diesel::update(user_custom_selectors::table.find(existing.id))
            .set((
                user_custom_selectors::selectors.eq(&selectors_json),
                user_custom_selectors::version.eq(request.version as i16),
                user_custom_selectors::updated_at.eq(now),
            ))
            .execute(&mut conn)?;
        existing.id
    } else {
        let id = Uuid::now_v7();
        diesel::insert_into(user_custom_selectors::table)
            .values(NewUserCustomSelectorsRow {
                id,
                user_id: auth.user.id,
                selectors: selectors_json,
                version: request.version as i16,
                updated_at: now,
                created_at: now,
            })
            .execute(&mut conn)?;
        id
    };

    insert_audit_log(
        &mut conn,
        Some(auth.user.id),
        "user.selectors.sync",
        "user_custom_selectors",
        Some(row_id),
        json!({ "count": request.selectors.len() }),
    )?;

    Ok(CustomSelectorsResponse {
        ok: true,
        selectors: request.selectors,
        version: request.version,
        updated_at: Some(now),
    })
}
