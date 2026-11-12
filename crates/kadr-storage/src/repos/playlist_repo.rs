use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use deadpool_sqlite::Pool;
use kadr_core::models::MediaItem;
use kadr_core::playlists::{Playlist, PlaylistSummary};
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Result, StorageError};
use crate::repos::media_item_repo::map_media_item_row;

fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn is_lib_visible(
    lib_id: &str,
    lib_is_private_opt: Option<i64>,
    unlocked_set: &HashSet<String>,
) -> bool {
    match lib_is_private_opt {
        Some(is_priv) => is_priv == 0 || unlocked_set.contains(lib_id),
        None => false, // Fail-closed if library lookup fails
    }
}

#[derive(Clone)]
pub struct PlaylistRepository {
    pool: Pool,
}

impl PlaylistRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    pub async fn create_playlist(
        &self,
        id: &str,
        owner_user_id: &str,
        name: &str,
        description: Option<&str>,
        is_public: bool,
    ) -> Result<Playlist> {
        let id = id.to_string();
        let owner_user_id = owner_user_id.to_string();
        let name = name.to_string();
        let description = description.map(ToOwned::to_owned);
        let now = now_epoch();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            c.execute(
                "INSERT INTO playlists (id, owner_user_id, name, description, is_public, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    owner_user_id,
                    name,
                    description,
                    if is_public { 1 } else { 0 },
                    now,
                    now,
                ],
            )?;

            Ok(Playlist {
                id,
                owner_user_id,
                name,
                description,
                is_public,
                created_at: now,
                updated_at: now,
            })
        })
        .await?
    }

    pub async fn get_playlist_raw(&self, playlist_id: &str) -> Result<Option<(Playlist, String)>> {
        let playlist_id = playlist_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let row = c
                .query_row(
                    "SELECT p.id, p.owner_user_id, p.name, p.description, p.is_public, p.created_at, p.updated_at, u.username
                     FROM playlists p
                     JOIN users u ON p.owner_user_id = u.id
                     WHERE p.id = ?1",
                    params![playlist_id],
                    |r| {
                        let pl = Playlist {
                            id: r.get(0)?,
                            owner_user_id: r.get(1)?,
                            name: r.get(2)?,
                            description: r.get(3)?,
                            is_public: r.get::<_, i64>(4)? != 0,
                            created_at: r.get(5)?,
                            updated_at: r.get(6)?,
                        };
                        let username: String = r.get(7)?;
                        Ok((pl, username))
                    },
                )
                .optional()?;
            Ok(row)
        })
        .await?
    }

    pub async fn update_playlist(
        &self,
        playlist_id: &str,
        name: Option<&str>,
        description: Option<Option<&str>>,
        is_public: Option<bool>,
    ) -> Result<Option<Playlist>> {
        let playlist_id = playlist_id.to_string();
        let name = name.map(ToOwned::to_owned);
        let description = description.map(|opt| opt.map(ToOwned::to_owned));
        let now = now_epoch();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            let existing = tx
                .query_row(
                    "SELECT id, owner_user_id, name, description, is_public, created_at, updated_at
                     FROM playlists WHERE id = ?1",
                    params![&playlist_id],
                    |r| {
                        Ok(Playlist {
                            id: r.get(0)?,
                            owner_user_id: r.get(1)?,
                            name: r.get(2)?,
                            description: r.get(3)?,
                            is_public: r.get::<_, i64>(4)? != 0,
                            created_at: r.get(5)?,
                            updated_at: r.get(6)?,
                        })
                    },
                )
                .optional()?;

            let Some(mut pl) = existing else {
                return Ok(None);
            };

            let mut changed = false;
            if let Some(n) = name {
                pl.name = n;
                changed = true;
            }
            if let Some(d) = description {
                pl.description = d;
                changed = true;
            }
            if let Some(pub_flag) = is_public {
                pl.is_public = pub_flag;
                changed = true;
            }

            if changed {
                pl.updated_at = now;
                tx.execute(
                    "UPDATE playlists SET name = ?1, description = ?2, is_public = ?3, updated_at = ?4 WHERE id = ?5",
                    params![
                        &pl.name,
                        &pl.description,
                        if pl.is_public { 1 } else { 0 },
                        pl.updated_at,
                        &pl.id,
                    ],
                )?;
            }

            tx.commit()?;
            Ok(Some(pl))
        })
        .await?
    }

    pub async fn delete_playlist(&self, playlist_id: &str) -> Result<bool> {
        let playlist_id = playlist_id.to_string();
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let deleted = c.execute("DELETE FROM playlists WHERE id = ?1", params![playlist_id])?;
            Ok(deleted > 0)
        })
        .await?
    }

    pub async fn get_playlist_summary_for_user(
        &self,
        playlist_id: &str,
        user_id: &str,
        is_admin: bool,
        unlocked_ids: &[String],
        contains_item_id: Option<i64>,
    ) -> Result<Option<PlaylistSummary>> {
        let playlist_id = playlist_id.to_string();
        let user_id = user_id.to_string();
        let unlocked_set: HashSet<String> = unlocked_ids.iter().cloned().collect();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let raw = c
                .query_row(
                    "SELECT p.id, p.owner_user_id, p.name, p.description, p.is_public, p.created_at, p.updated_at, u.username
                     FROM playlists p
                     JOIN users u ON p.owner_user_id = u.id
                     WHERE p.id = ?1",
                    params![&playlist_id],
                    |r| {
                        Ok((
                            Playlist {
                                id: r.get(0)?,
                                owner_user_id: r.get(1)?,
                                name: r.get(2)?,
                                description: r.get(3)?,
                                is_public: r.get::<_, i64>(4)? != 0,
                                created_at: r.get(5)?,
                                updated_at: r.get(6)?,
                            },
                            r.get::<_, String>(7)?,
                        ))
                    },
                )
                .optional()?;

            let Some((pl, owner_username)) = raw else {
                return Ok(None);
            };

            if pl.owner_user_id != user_id && !pl.is_public && !is_admin {
                return Ok(None);
            }

            let summary = build_summary_in_conn(
                c,
                pl,
                owner_username,
                &user_id,
                is_admin,
                &unlocked_set,
                contains_item_id,
            )?;
            Ok(Some(summary))
        })
        .await?
    }

    pub async fn list_playlists_for_user(
        &self,
        user_id: &str,
        is_admin: bool,
        unlocked_ids: &[String],
        contains_item_id: Option<i64>,
    ) -> Result<Vec<PlaylistSummary>> {
        let user_id = user_id.to_string();
        let unlocked_set: HashSet<String> = unlocked_ids.iter().cloned().collect();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare(
                "SELECT p.id, p.owner_user_id, p.name, p.description, p.is_public, p.created_at, p.updated_at, u.username
                 FROM playlists p
                 JOIN users u ON p.owner_user_id = u.id
                 WHERE p.owner_user_id = ?1 OR p.is_public = 1
                 ORDER BY (p.owner_user_id = ?1) DESC, p.updated_at DESC, p.id ASC",
            )?;
            let rows = stmt.query_map(params![&user_id], |r| {
                Ok((
                    Playlist {
                        id: r.get(0)?,
                        owner_user_id: r.get(1)?,
                        name: r.get(2)?,
                        description: r.get(3)?,
                        is_public: r.get::<_, i64>(4)? != 0,
                        created_at: r.get(5)?,
                        updated_at: r.get(6)?,
                    },
                    r.get::<_, String>(7)?,
                ))
            })?;

            let mut playlists = Vec::new();
            for row in rows {
                let (pl, owner_username) = row?;
                playlists.push(build_summary_in_conn(
                    c,
                    pl,
                    owner_username,
                    &user_id,
                    is_admin,
                    &unlocked_set,
                    contains_item_id,
                )?);
            }
            Ok(playlists)
        })
        .await?
    }

    pub async fn get_playlist_items_with_privacy(
        &self,
        playlist_id: &str,
        unlocked_ids: &[String],
    ) -> Result<(Vec<(i64, MediaItem)>, usize)> {
        let playlist_id = playlist_id.to_string();
        let unlocked_set: HashSet<String> = unlocked_ids.iter().cloned().collect();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let mut stmt = c.prepare(
                "SELECT m.id, m.library_id, m.item_type, m.title, m.original_title, m.release_year,
                        m.duration_seconds, m.added_at, m.file_path, m.file_name, m.file_size,
                        m.resolution, m.video_codec, m.audio_codec, m.audio_channels, m.container, m.metadata,
                        pi.added_at, l.is_private
                 FROM playlist_items pi
                 JOIN media_items m ON pi.media_item_id = m.id
                 LEFT JOIN libraries l ON m.library_id = l.id
                 WHERE pi.playlist_id = ?1
                 ORDER BY pi.position ASC, pi.added_at ASC",
            )?;

            let rows = stmt.query_map(params![playlist_id], |row| {
                let item = map_media_item_row(row)?;
                let item_added_at: i64 = row.get(17)?;
                let lib_is_private_opt: Option<i64> = row.get(18)?;
                Ok((item, item_added_at, lib_is_private_opt))
            })?;

            let mut visible = Vec::new();
            let mut locked_count = 0usize;
            for row in rows {
                let (item, item_added_at, lib_is_private_opt) = row?;
                if is_lib_visible(&item.library_id, lib_is_private_opt, &unlocked_set) {
                    visible.push((item_added_at, item));
                } else {
                    locked_count += 1;
                }
            }

            Ok((visible, locked_count))
        })
        .await?
    }

    pub async fn add_items(
        &self,
        playlist_id: &str,
        media_item_ids: &[i64],
    ) -> Result<(usize, usize)> {
        let playlist_id = playlist_id.to_string();
        let ids = media_item_ids.to_vec();
        let now = now_epoch();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            let exists: Option<i64> = tx
                .query_row(
                    "SELECT 1 FROM playlists WHERE id = ?1",
                    params![&playlist_id],
                    |r| r.get(0),
                )
                .optional()?;
            if exists.is_none() {
                return Err(StorageError::NotFound(format!(
                    "Playlist {playlist_id} not found"
                )));
            }

            let mut next_pos: i64 = tx.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM playlist_items WHERE playlist_id = ?1",
                params![&playlist_id],
                |r| r.get(0),
            )?;

            let mut added = 0usize;
            let mut skipped = 0usize;
            for media_id in ids {
                let inserted = tx.execute(
                    "INSERT OR IGNORE INTO playlist_items (playlist_id, media_item_id, position, added_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![&playlist_id, media_id, next_pos, now],
                )?;
                if inserted > 0 {
                    added += 1;
                    next_pos += 1;
                } else {
                    skipped += 1;
                }
            }

            if added > 0 {
                tx.execute(
                    "UPDATE playlists SET updated_at = ?1 WHERE id = ?2",
                    params![now, &playlist_id],
                )?;
            }

            tx.commit()?;
            Ok((added, skipped))
        })
        .await?
    }

    pub async fn remove_item(&self, playlist_id: &str, media_item_id: i64) -> Result<bool> {
        let playlist_id = playlist_id.to_string();
        let now = now_epoch();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            let deleted = tx.execute(
                "DELETE FROM playlist_items WHERE playlist_id = ?1 AND media_item_id = ?2",
                params![&playlist_id, media_item_id],
            )?;
            if deleted == 0 {
                return Ok(false);
            }

            let remaining: Vec<i64> = {
                let mut stmt = tx.prepare(
                    "SELECT media_item_id FROM playlist_items
                     WHERE playlist_id = ?1 ORDER BY position ASC, added_at ASC",
                )?;
                let rows = stmt.query_map(params![&playlist_id], |r| r.get::<_, i64>(0))?;
                let mut ids = Vec::new();
                for r in rows {
                    ids.push(r?);
                }
                ids
            };

            for (idx, item_id) in remaining.into_iter().enumerate() {
                tx.execute(
                    "UPDATE playlist_items SET position = ?1 WHERE playlist_id = ?2 AND media_item_id = ?3",
                    params![idx as i64, &playlist_id, item_id],
                )?;
            }

            tx.execute(
                "UPDATE playlists SET updated_at = ?1 WHERE id = ?2",
                params![now, &playlist_id],
            )?;

            tx.commit()?;
            Ok(true)
        })
        .await?
    }

    pub async fn reorder_visible_items(
        &self,
        playlist_id: &str,
        ordered_visible_ids: &[i64],
        unlocked_ids: &[String],
    ) -> Result<()> {
        let playlist_id = playlist_id.to_string();
        let ordered_visible_ids = ordered_visible_ids.to_vec();
        let unlocked_set: HashSet<String> = unlocked_ids.iter().cloned().collect();
        let now = now_epoch();

        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            let tx = c.transaction()?;
            let exists: Option<i64> = tx
                .query_row(
                    "SELECT 1 FROM playlists WHERE id = ?1",
                    params![&playlist_id],
                    |r| r.get(0),
                )
                .optional()?;
            if exists.is_none() {
                return Err(StorageError::NotFound(format!(
                    "Playlist {playlist_id} not found"
                )));
            }

            let mut visible_ids = Vec::new();
            let mut locked_ids = Vec::new();

            {
                let mut stmt = tx.prepare(
                    "SELECT pi.media_item_id, m.library_id, l.is_private
                     FROM playlist_items pi
                     JOIN media_items m ON pi.media_item_id = m.id
                     LEFT JOIN libraries l ON m.library_id = l.id
                     WHERE pi.playlist_id = ?1
                     ORDER BY pi.position ASC, pi.added_at ASC",
                )?;
                let rows = stmt.query_map(params![&playlist_id], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                    ))
                })?;
                for r in rows {
                    let (media_id, lib_id, is_private_opt) = r?;
                    if is_lib_visible(&lib_id, is_private_opt, &unlocked_set) {
                        visible_ids.push(media_id);
                    } else {
                        locked_ids.push(media_id);
                    }
                }
            }

            let expected_set: HashSet<i64> = visible_ids.iter().copied().collect();
            let provided_set: HashSet<i64> = ordered_visible_ids.iter().copied().collect();
            if ordered_visible_ids.len() != visible_ids.len()
                || provided_set.len() != ordered_visible_ids.len()
                || provided_set != expected_set
            {
                return Err(StorageError::InvalidInput(
                    "item_ids must be an exact permutation of visible playlist items".to_string(),
                ));
            }

            let final_order: Vec<i64> = ordered_visible_ids
                .into_iter()
                .chain(locked_ids)
                .collect();

            for (idx, media_id) in final_order.into_iter().enumerate() {
                tx.execute(
                    "UPDATE playlist_items SET position = ?1 WHERE playlist_id = ?2 AND media_item_id = ?3",
                    params![idx as i64, &playlist_id, media_id],
                )?;
            }

            tx.execute(
                "UPDATE playlists SET updated_at = ?1 WHERE id = ?2",
                params![now, &playlist_id],
            )?;

            tx.commit()?;
            Ok(())
        })
        .await?
    }
}

fn build_summary_in_conn(
    c: &Connection,
    pl: Playlist,
    owner_username: String,
    user_id: &str,
    is_admin: bool,
    unlocked_set: &HashSet<String>,
    contains_item_id: Option<i64>,
) -> rusqlite::Result<PlaylistSummary> {
    let mut stmt = c.prepare(
        "SELECT m.id, m.library_id, m.duration_seconds, json_extract(m.metadata, '$.poster_path'), l.is_private
         FROM playlist_items pi
         JOIN media_items m ON pi.media_item_id = m.id
         LEFT JOIN libraries l ON m.library_id = l.id
         WHERE pi.playlist_id = ?1
         ORDER BY pi.position ASC, pi.added_at ASC",
    )?;
    let rows = stmt.query_map(params![&pl.id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<i64>>(4)?,
        ))
    })?;

    let mut visible_item_count = 0usize;
    let mut locked_item_count = 0usize;
    let mut total_duration_seconds = 0i64;
    let mut cover_poster_urls = Vec::new();
    let mut found_queried_visible = false;

    for r in rows {
        let (media_id, lib_id, dur, poster_path, is_private_opt) = r?;
        if is_lib_visible(&lib_id, is_private_opt, unlocked_set) {
            visible_item_count += 1;
            total_duration_seconds += dur.max(0);
            if cover_poster_urls.len() < 4
                && poster_path.as_deref().is_some_and(|p| !p.trim().is_empty())
            {
                cover_poster_urls.push(format!("/api/v1/artwork/{media_id}/poster"));
            }
            if contains_item_id == Some(media_id) {
                found_queried_visible = true;
            }
        } else {
            locked_item_count += 1;
        }
    }

    let contains_queried_item = contains_item_id.map(|_| found_queried_visible);
    let is_editable = pl.owner_user_id == user_id || is_admin;

    Ok(PlaylistSummary {
        id: pl.id,
        name: pl.name,
        description: pl.description,
        owner_user_id: pl.owner_user_id,
        owner_username,
        is_public: pl.is_public,
        is_editable,
        visible_item_count,
        locked_item_count,
        total_duration_seconds,
        cover_poster_urls,
        created_at: pl.created_at,
        updated_at: pl.updated_at,
        contains_queried_item,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use tempfile::TempDir;

    use crate::error::StorageError;
    use crate::pool::{create_in_memory_pool, initialize_database};
    use crate::repos::{
        LibraryRepository, MediaItemRepository, PlaylistRepository, UserRepository,
    };
    use kadr_core::models::{
        Library, MediaItem, MediaMetadata, MediaType, TechnicalInfo, User, UserRole,
    };

    fn sample_item(
        lib: &str,
        title: &str,
        path: PathBuf,
        dur: i64,
        poster: Option<&str>,
    ) -> MediaItem {
        MediaItem {
            id: None,
            library_id: lib.to_string(),
            item_type: MediaType::Movie,
            title: title.to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: 1000,
            file_path: path.clone(),
            file_name: path.file_name().unwrap().to_string_lossy().to_string(),
            file_size: 1024,
            technical: TechnicalInfo {
                duration_seconds: dur,
                ..Default::default()
            },
            metadata: MediaMetadata {
                poster_path: poster.map(ToOwned::to_owned),
                ..Default::default()
            },
        }
    }

    async fn insert_item(repo: &MediaItemRepository, item: MediaItem) -> i64 {
        repo.upsert_batch(std::slice::from_ref(&item))
            .await
            .unwrap();
        repo.find_by_path(&item.file_path)
            .await
            .unwrap()
            .unwrap()
            .id
            .unwrap()
    }

    async fn raw_positions(pool: &deadpool_sqlite::Pool, pl_id: &str) -> Vec<(i64, i64)> {
        let pl = pl_id.to_string();
        let conn = pool.get().await.unwrap();
        conn.interact(move |c| {
            let mut stmt = c
                .prepare(
                    "SELECT media_item_id, position FROM playlist_items
                     WHERE playlist_id = ?1 ORDER BY position ASC",
                )
                .unwrap();
            stmt.query_map([pl], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
                .unwrap()
                .map(|r| r.unwrap())
                .collect::<Vec<_>>()
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn playlist_crud_privacy_reorder_and_dedup_safety() {
        let pool = create_in_memory_pool().unwrap();
        initialize_database(&pool).await.unwrap();

        let user_repo = UserRepository::new(pool.clone());
        let lib_repo = LibraryRepository::new(pool.clone());
        let media_repo = MediaItemRepository::new(pool.clone());
        let playlist_repo = PlaylistRepository::new(pool.clone());

        user_repo
            .create(&User {
                id: "u-1".to_string(),
                username: "alex".to_string(),
                pin_hash: "h".to_string(),
                role: UserRole::Standard,
                created_at: 100,
            })
            .await
            .unwrap();
        user_repo
            .create(&User {
                id: "u-2".to_string(),
                username: "sam".to_string(),
                pin_hash: "h".to_string(),
                role: UserRole::Standard,
                created_at: 100,
            })
            .await
            .unwrap();

        lib_repo
            .insert(&Library {
                id: "lib-pub".to_string(),
                name: "Public".to_string(),
                path: PathBuf::from("/pub"),
                media_type: MediaType::Movie,
                is_private: false,
                created_at: 100,
                ..Default::default()
            })
            .await
            .unwrap();

        lib_repo
            .insert(&Library {
                id: "lib-priv".to_string(),
                name: "Private".to_string(),
                path: PathBuf::from("/priv"),
                media_type: MediaType::Movie,
                is_private: true,
                pin_hash: Some("pin".to_string()),
                created_at: 100,
                ..Default::default()
            })
            .await
            .unwrap();

        let tmp = TempDir::new().unwrap();
        let real_file = tmp.path().join("canon_movie.mkv");
        let symlink_file = tmp.path().join("alias_movie.mkv");
        let real_file_b = tmp.path().join("canon_movie_b.mkv");
        let symlink_file_b = tmp.path().join("alias_movie_b.mkv");
        std::fs::write(&real_file, b"video").unwrap();
        std::os::unix::fs::symlink(&real_file, &symlink_file).unwrap();
        std::fs::write(&real_file_b, b"video-b").unwrap();
        std::os::unix::fs::symlink(&real_file_b, &symlink_file_b).unwrap();

        let id1 = insert_item(
            &media_repo,
            sample_item("lib-pub", "One", real_file.clone(), 3600, Some("/p1.jpg")),
        )
        .await;
        let id_priv = insert_item(
            &media_repo,
            sample_item(
                "lib-priv",
                "Secret",
                PathBuf::from("/priv/s.mkv"),
                5000,
                Some("/ps.jpg"),
            ),
        )
        .await;
        let id2 = insert_item(
            &media_repo,
            sample_item(
                "lib-pub",
                "Two",
                PathBuf::from("/pub/two.mkv"),
                1800,
                Some("/p2.jpg"),
            ),
        )
        .await;
        let id_dup = insert_item(
            &media_repo,
            sample_item("lib-pub", "One Alias", symlink_file, 3600, None),
        )
        .await;
        let id_b_keep = insert_item(
            &media_repo,
            sample_item("lib-pub", "B Keep", real_file_b, 1200, None),
        )
        .await;
        let id_b_del = insert_item(
            &media_repo,
            sample_item("lib-pub", "B Alias", symlink_file_b, 1200, None),
        )
        .await;

        let created = playlist_repo
            .create_playlist("pl-1", "u-1", "Weekend", Some("Queue"), false)
            .await
            .unwrap();
        assert_eq!(created.name, "Weekend");
        assert_eq!(created.description.as_deref(), Some("Queue"));
        assert!(!created.is_public);

        // pl-2 only references id_b_del (so deduplicate must migrate id_b_del -> id_b_keep without losing it)
        playlist_repo
            .create_playlist("pl-2", "u-2", "Sam Public", None, true)
            .await
            .unwrap();
        playlist_repo
            .add_items("pl-2", &[id2, id_b_del])
            .await
            .unwrap();

        let (added, skipped) = playlist_repo
            .add_items("pl-1", &[id1, id_priv, id2, id1, id_dup])
            .await
            .unwrap();
        assert_eq!((added, skipped), (4, 1));

        // Update metadata & visibility
        let updated = playlist_repo
            .update_playlist("pl-1", Some("Weekend Updated"), Some(None), Some(false))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.name, "Weekend Updated");
        assert!(updated.description.is_none());

        // list_playlists_for_user returns own playlists first, then public playlists of others
        let u1_list = playlist_repo
            .list_playlists_for_user("u-1", false, &[], Some(id_priv))
            .await
            .unwrap();
        assert_eq!(u1_list.len(), 2);
        assert_eq!(u1_list[0].id, "pl-1");
        assert!(u1_list[0].is_editable);
        assert_eq!(u1_list[0].contains_queried_item, Some(false)); // locked item returns Some(false)
        assert_eq!(u1_list[1].id, "pl-2");
        assert!(!u1_list[1].is_editable);

        // Locked library isolation on summary
        let sum_locked = playlist_repo
            .get_playlist_summary_for_user("pl-1", "u-1", false, &[], Some(id_priv))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(sum_locked.visible_item_count, 3);
        assert_eq!(sum_locked.locked_item_count, 1);
        assert_eq!(sum_locked.total_duration_seconds, 9000);
        assert_eq!(sum_locked.contains_queried_item, Some(false));
        assert!(!sum_locked
            .cover_poster_urls
            .iter()
            .any(|u| u.contains(&id_priv.to_string())));

        // Unlocked contains_item_id returns Some(true)
        let sum_unlocked = playlist_repo
            .get_playlist_summary_for_user(
                "pl-1",
                "u-1",
                false,
                &["lib-priv".to_string()],
                Some(id_priv),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(sum_unlocked.contains_queried_item, Some(true));

        // Invalid reorder rejected (wrong count or duplicate IDs or locked ID passed while locked)
        let bad = playlist_repo
            .reorder_visible_items("pl-1", &[id2], &[])
            .await;
        assert!(matches!(bad, Err(StorageError::InvalidInput(_))));
        let bad_dup = playlist_repo
            .reorder_visible_items("pl-1", &[id2, id2, id1], &[])
            .await;
        assert!(matches!(bad_dup, Err(StorageError::InvalidInput(_))));

        // Valid reorder while locked preserves id_priv at tail (positions 0..2 visible, 3 locked)
        playlist_repo
            .reorder_visible_items("pl-1", &[id2, id_dup, id1], &[])
            .await
            .unwrap();
        assert_eq!(
            raw_positions(&pool, "pl-1").await,
            vec![(id2, 0), (id_dup, 1), (id1, 2), (id_priv, 3)]
        );

        // Deduplicate merges id_dup -> id1 (already in pl-1) AND id_b_del -> id_b_keep (not yet in pl-2)
        // and re-compacts dense 0..N-1 positions for both playlists
        let merged = media_repo.deduplicate_media_items().await.unwrap();
        assert_eq!(merged, 2);

        assert_eq!(
            raw_positions(&pool, "pl-1").await,
            vec![(id2, 0), (id1, 1), (id_priv, 2)]
        );
        assert_eq!(
            raw_positions(&pool, "pl-2").await,
            vec![(id2, 0), (id_b_keep, 1)]
        );

        let (unlocked_items, locked_cnt) = playlist_repo
            .get_playlist_items_with_privacy("pl-1", &["lib-priv".to_string()])
            .await
            .unwrap();
        assert_eq!(locked_cnt, 0);
        let ids: Vec<i64> = unlocked_items.iter().map(|(_, m)| m.id.unwrap()).collect();
        assert_eq!(ids, vec![id2, id1, id_priv]);

        // Remove middle item -> remaining positions remain dense 0..N-1
        assert!(playlist_repo.remove_item("pl-1", id1).await.unwrap());
        assert!(!playlist_repo.remove_item("pl-1", id1).await.unwrap());
        assert_eq!(
            raw_positions(&pool, "pl-1").await,
            vec![(id2, 0), (id_priv, 1)]
        );

        // Delete playlist cascades playlist_items
        assert!(playlist_repo.delete_playlist("pl-1").await.unwrap());
        assert!(!playlist_repo.delete_playlist("pl-1").await.unwrap());
        assert!(raw_positions(&pool, "pl-1").await.is_empty());
    }

    #[tokio::test]
    async fn playlist_cascade_on_user_and_media_delete_and_cover_limit() {
        let pool = create_in_memory_pool().unwrap();
        initialize_database(&pool).await.unwrap();

        let user_repo = UserRepository::new(pool.clone());
        let lib_repo = LibraryRepository::new(pool.clone());
        let media_repo = MediaItemRepository::new(pool.clone());
        let playlist_repo = PlaylistRepository::new(pool.clone());

        user_repo
            .create(&User {
                id: "u-cascade".to_string(),
                username: "cascade".to_string(),
                pin_hash: "h".to_string(),
                role: UserRole::Standard,
                created_at: 100,
            })
            .await
            .unwrap();

        lib_repo
            .insert(&Library {
                id: "lib-1".to_string(),
                name: "Movies".to_string(),
                path: PathBuf::from("/movies"),
                media_type: MediaType::Movie,
                is_private: false,
                created_at: 100,
                ..Default::default()
            })
            .await
            .unwrap();

        let mut ids = Vec::new();
        for idx in 1..=6 {
            let p = PathBuf::from(format!("/movies/m{idx}.mkv"));
            let poster = format!("/art/m{idx}.jpg");
            ids.push(
                insert_item(
                    &media_repo,
                    sample_item("lib-1", &format!("M{idx}"), p, 1000, Some(&poster)),
                )
                .await,
            );
        }

        playlist_repo
            .create_playlist("pl-c", "u-cascade", "Cover Test", None, true)
            .await
            .unwrap();
        playlist_repo.add_items("pl-c", &ids).await.unwrap();

        let summary = playlist_repo
            .get_playlist_summary_for_user("pl-c", "u-cascade", false, &[], None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(summary.visible_item_count, 6);
        assert_eq!(summary.cover_poster_urls.len(), 4);
        assert_eq!(
            summary.cover_poster_urls[0],
            format!("/api/v1/artwork/{}/poster", ids[0])
        );

        // Deleting a media item cascades from playlist_items
        media_repo.delete_by_path("/movies/m1.mkv").await.unwrap();
        let summary_after_media_del = playlist_repo
            .get_playlist_summary_for_user("pl-c", "u-cascade", false, &[], None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(summary_after_media_del.visible_item_count, 5);

        // Deleting user cascades playlists and playlist_items
        {
            let conn = pool.get().await.unwrap();
            conn.interact(|c| {
                c.execute("DELETE FROM users WHERE id = 'u-cascade'", [])
                    .unwrap();
            })
            .await
            .unwrap();
        }

        assert!(playlist_repo
            .get_playlist_raw("pl-c")
            .await
            .unwrap()
            .is_none());
        assert!(raw_positions(&pool, "pl-c").await.is_empty());
    }
}
