use std::collections::HashSet;

use deadpool_sqlite::Pool;
use kadr_core::ast::WidgetFilterConfig;
use kadr_core::models::{MediaItem, MediaType};
use rusqlite::params;
use rusqlite::types::Value;

use crate::error::Result;
use crate::repos::filter_compiler::media_type_db_str;
use crate::repos::widget_queries::{
    build_filter_envelope_for_user, current_epoch_secs, map_media_item_row,
    resolve_effective_library_id, resolve_sort_clause, SELECT_JOINED_COLUMNS,
};

#[derive(Clone)]
pub struct FavoritesRepository {
    pool: Pool,
}

pub type FavoritesRepo = FavoritesRepository;

impl FavoritesRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    pub async fn add_favorite(
        &self,
        user_id: &str,
        media_item_id: i64,
        now_epoch: i64,
    ) -> Result<()> {
        let uid = user_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            c.execute(
                "INSERT OR IGNORE INTO user_favorites (user_id, media_item_id, favorited_at)
                 VALUES (?1, ?2, ?3)",
                params![uid, media_item_id, now_epoch],
            )?;
            Ok(())
        })
        .await?
    }

    pub async fn remove_favorite(&self, user_id: &str, media_item_id: i64) -> Result<bool> {
        let uid = user_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let deleted = c.execute(
                "DELETE FROM user_favorites WHERE user_id = ?1 AND media_item_id = ?2",
                params![uid, media_item_id],
            )?;
            Ok(deleted > 0)
        })
        .await?
    }

    pub async fn is_favorite(&self, user_id: &str, media_item_id: i64) -> Result<bool> {
        if user_id.is_empty() {
            return Ok(false);
        }
        let uid = user_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let exists: i64 = c.query_row(
                "SELECT EXISTS (SELECT 1 FROM user_favorites WHERE user_id = ?1 AND media_item_id = ?2)",
                params![uid, media_item_id],
                |row| row.get(0),
            )?;
            Ok(exists != 0)
        })
        .await?
    }

    pub async fn get_favorited_ids(&self, user_id: &str, item_ids: &[i64]) -> Result<HashSet<i64>> {
        if user_id.is_empty() || item_ids.is_empty() {
            return Ok(HashSet::new());
        }
        let uid = user_id.to_string();
        let ids = item_ids.to_vec();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut out = HashSet::new();
            for chunk in ids.chunks(500) {
                let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
                let sql = format!(
                    "SELECT media_item_id FROM user_favorites WHERE user_id = ? AND media_item_id IN ({placeholders})"
                );
                let mut sql_params = Vec::with_capacity(1 + chunk.len());
                sql_params.push(Value::Text(uid.clone()));
                for id in chunk {
                    sql_params.push(Value::Integer(*id));
                }
                let mut stmt = c.prepare(&sql)?;
                let rows = stmt.query_map(rusqlite::params_from_iter(sql_params), |row| {
                    row.get::<_, i64>(0)
                })?;
                for row in rows {
                    out.insert(row?);
                }
            }
            Ok(out)
        })
        .await?
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn list_favorites(
        &self,
        user_id: &str,
        library_id: Option<&str>,
        item_type: Option<MediaType>,
        limit: u32,
        offset: u32,
        sort: Option<&str>,
        unlocked_ids: &[String],
        filters: Option<&WidgetFilterConfig>,
    ) -> Result<(Vec<(i64, MediaItem)>, u64, u64)> {
        let uid = user_id.to_string();
        let filters_owned = filters.cloned();
        let lib_id = resolve_effective_library_id(
            library_id.map(str::trim).filter(|s| !s.is_empty()),
            filters_owned.as_ref(),
        );
        let item_type_str = item_type.map(media_type_db_str);
        let (order_clause, sort_needs_playback_join) =
            match sort.map(|s| s.trim().to_lowercase()).as_deref() {
                Some("favorited_at:asc") | Some("favorited_at_asc") => {
                    ("uf.favorited_at ASC, uf.media_item_id ASC", false)
                }
                Some("favorited_at:desc") | Some("favorited_at_desc") | Some("") | None => {
                    ("uf.favorited_at DESC, uf.media_item_id DESC", false)
                }
                _ => resolve_sort_clause(sort, "uf.favorited_at DESC, uf.media_item_id DESC"),
            };
        let unlocked = unlocked_ids.to_vec();
        let now = current_epoch_secs();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let locked_count: i64 = if filters_owned.as_ref().is_some_and(|f| f.exclude_private) {
                0
            } else {
                let mut locked_where =
                    vec!["uf.user_id = ?".to_string(), "l.is_private = 1".to_string()];
                let mut locked_params = vec![Value::Text(uid.clone())];

                if !unlocked.is_empty() {
                    let ph = unlocked.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
                    locked_where.push(format!("l.id NOT IN ({ph})"));
                    for id in &unlocked {
                        locked_params.push(Value::Text(id.clone()));
                    }
                }
                if let Some(ref lid) = lib_id {
                    locked_where.push("m.library_id = ?".to_string());
                    locked_params.push(Value::Text(lid.clone()));
                }
                if let Some(ref f) = filters_owned {
                    if !f.exclude_library_ids.is_empty() {
                        let ph = f
                            .exclude_library_ids
                            .iter()
                            .map(|_| "?")
                            .collect::<Vec<_>>()
                            .join(", ");
                        locked_where.push(format!("m.library_id NOT IN ({ph})"));
                        for excl_id in &f.exclude_library_ids {
                            locked_params.push(Value::Text(excl_id.clone()));
                        }
                    }
                }
                if let Some(it) = item_type_str {
                    locked_where.push("m.item_type = ?".to_string());
                    locked_params.push(Value::Text(it.to_string()));
                }

                let locked_sql = format!(
                    "SELECT COUNT(*) \
                     FROM user_favorites uf \
                     JOIN media_items m ON m.id = uf.media_item_id \
                     JOIN libraries l ON l.id = m.library_id \
                     WHERE {}",
                    locked_where.join(" AND ")
                );
                c.query_row(
                    &locked_sql,
                    rusqlite::params_from_iter(locked_params),
                    |row| row.get(0),
                )?
            };

            let envelope =
                build_filter_envelope_for_user(filters_owned.as_ref(), Some(&uid), &unlocked, now);
            let needs_join = envelope.needs_playback_join || sort_needs_playback_join;

            let mut base_params = Vec::new();
            let playback_join = if needs_join {
                base_params.push(Value::Text(uid.clone()));
                "LEFT JOIN user_playback_states ups ON ups.media_item_id = m.id AND ups.user_id = ?"
            } else {
                ""
            };

            base_params.push(Value::Text(uid));
            let lib_cond = if let Some(lid) = lib_id {
                base_params.push(Value::Text(lid));
                "AND m.library_id = ?"
            } else {
                ""
            };
            let type_cond = if let Some(it) = item_type_str {
                base_params.push(Value::Text(it.to_string()));
                "AND m.item_type = ?"
            } else {
                ""
            };
            base_params.extend(envelope.params);

            let count_sql = format!(
                "SELECT COUNT(*) \
                 FROM user_favorites uf \
                 JOIN media_items m ON m.id = uf.media_item_id \
                 JOIN libraries l ON l.id = m.library_id \
                 {playback_join} \
                 WHERE uf.user_id = ? {lib_cond} {type_cond} AND {}",
                envelope.sql
            );
            let visible_total: i64 = c.query_row(
                &count_sql,
                rusqlite::params_from_iter(base_params.iter().cloned()),
                |row| row.get(0),
            )?;

            let mut select_params = base_params;
            select_params.push(Value::Integer(limit as i64));
            select_params.push(Value::Integer(offset as i64));

            // SELECT_JOINED_COLUMNS spans columns 0..=18; uf.favorited_at is column 19.
            let select_sql = format!(
                "SELECT {SELECT_JOINED_COLUMNS}, uf.favorited_at \
                 FROM user_favorites uf \
                 JOIN media_items m ON m.id = uf.media_item_id \
                 JOIN libraries l ON l.id = m.library_id \
                 {playback_join} \
                 WHERE uf.user_id = ? {lib_cond} {type_cond} AND {} \
                 ORDER BY {order_clause} \
                 LIMIT ? OFFSET ?",
                envelope.sql
            );
            let mut stmt = c.prepare(&select_sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(select_params), |row| {
                let item = map_media_item_row(row)?;
                let favorited_at: i64 = row.get(19)?;
                Ok((favorited_at, item))
            })?;

            let mut items = Vec::new();
            for row in rows {
                items.push(row?);
            }

            Ok((items, visible_total as u64, locked_count as u64))
        })
        .await?
    }
}
