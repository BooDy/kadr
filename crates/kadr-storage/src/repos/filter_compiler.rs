use kadr_core::filter_ast::{
    ArrayMatch, FilterCondition, FilterNode, FloatComparison, IntComparison, LogicalOp, TextMatch,
    TimeComparison, WatchStateMatch,
};
use kadr_core::models::{MediaType, WatchState};
use rusqlite::types::Value;

#[derive(Debug, Clone, Default)]
pub struct CompiledFilter {
    pub sql: String,
    pub params: Vec<Value>,
    pub needs_playback_join: bool,
    pub references_item_type: bool,
}

fn escape_like(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch == '\\' || ch == '%' || ch == '_' {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn f32_to_sql_f64(v: f32) -> f64 {
    v.to_string().parse::<f64>().unwrap_or(v as f64)
}

pub(crate) fn media_type_db_str(mt: MediaType) -> &'static str {
    match mt {
        MediaType::Movie => "movie",
        MediaType::Show => "show",
        MediaType::Season => "season",
        MediaType::Episode => "episode",
        MediaType::Anime => "anime",
        MediaType::Music => "music",
        MediaType::HomeVideos => "home_videos",
        MediaType::Audiobook => "audiobook",
        MediaType::LiveChannel => "live_channel",
        MediaType::Unknown => "unknown",
    }
}

fn watch_state_db_str(ws: WatchState) -> &'static str {
    match ws {
        WatchState::Unwatched => "unwatched",
        WatchState::InProgress => "in_progress",
        WatchState::Completed => "completed",
    }
}

pub fn compile_filter_node(node: &FilterNode, now_epoch: i64) -> CompiledFilter {
    compile_filter_node_for_user(node, None, now_epoch)
}

pub fn compile_filter_node_for_user(
    node: &FilterNode,
    user_id: Option<&str>,
    now_epoch: i64,
) -> CompiledFilter {
    let mut out = CompiledFilter::default();
    out.sql = compile_inner(node, user_id, now_epoch, &mut out);
    out
}

fn compile_inner(
    node: &FilterNode,
    user_id: Option<&str>,
    now_epoch: i64,
    state: &mut CompiledFilter,
) -> String {
    match node {
        FilterNode::Group {
            op,
            negated,
            children,
        } => {
            if children.is_empty() {
                return if *negated {
                    "0 = 1".to_string()
                } else {
                    "1 = 1".to_string()
                };
            }
            let joiner = match op {
                LogicalOp::And => " AND ",
                LogicalOp::Or => " OR ",
            };
            let parts: Vec<String> = children
                .iter()
                .map(|c| compile_inner(c, user_id, now_epoch, state))
                .collect();
            let grouped = format!("({})", parts.join(joiner));
            if *negated {
                format!("NOT COALESCE({grouped}, 0)")
            } else {
                grouped
            }
        }
        FilterNode::Rule { negated, condition } => {
            let expr = compile_condition(condition, user_id, now_epoch, state);
            if *negated {
                format!("NOT COALESCE(({expr}), 0)")
            } else {
                expr
            }
        }
    }
}

fn compile_text_match(expr: &str, matcher: &TextMatch, params: &mut Vec<Value>) -> String {
    match matcher {
        TextMatch::IsEmpty => format!("({expr} IS NULL OR TRIM({expr}) = '')"),
        TextMatch::Eq { value } => {
            params.push(Value::Text(value.trim().to_lowercase()));
            format!("(LOWER({expr}) = ?)")
        }
        TextMatch::In { values } => {
            if values.is_empty() {
                return "0 = 1".to_string();
            }
            let ph = values.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            for v in values {
                params.push(Value::Text(v.trim().to_lowercase()));
            }
            format!("(LOWER({expr}) IN ({ph}))")
        }
        TextMatch::Contains { value } => {
            let escaped = escape_like(&value.trim().to_lowercase());
            params.push(Value::Text(format!("%{escaped}%")));
            format!("(LOWER({expr}) LIKE ? ESCAPE '\\')")
        }
        TextMatch::StartsWith { value } => {
            let escaped = escape_like(&value.trim().to_lowercase());
            params.push(Value::Text(format!("{escaped}%")));
            format!("(LOWER({expr}) LIKE ? ESCAPE '\\')")
        }
    }
}

fn compile_array_match(json_path: &str, matcher: &ArrayMatch, params: &mut Vec<Value>) -> String {
    let arr_expr = format!("json_extract(m.metadata, '{json_path}')");
    match matcher {
        ArrayMatch::IsEmpty => {
            format!("({arr_expr} IS NULL OR json_array_length({arr_expr}) = 0)")
        }
        ArrayMatch::ContainsAny { values } => {
            if values.is_empty() {
                return "0 = 1".to_string();
            }
            let ph = values.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            for v in values {
                params.push(Value::Text(v.trim().to_lowercase()));
            }
            format!(
                "({arr_expr} IS NOT NULL AND EXISTS (SELECT 1 FROM json_each({arr_expr}) WHERE LOWER(TRIM(value)) IN ({ph})))"
            )
        }
        ArrayMatch::ContainsAll { values } => {
            if values.is_empty() {
                return "1 = 1".to_string();
            }
            let mut subclauses = Vec::with_capacity(values.len());
            for v in values {
                params.push(Value::Text(v.trim().to_lowercase()));
                subclauses.push(format!(
                    "EXISTS (SELECT 1 FROM json_each({arr_expr}) WHERE LOWER(TRIM(value)) = ?)"
                ));
            }
            format!("({arr_expr} IS NOT NULL AND {})", subclauses.join(" AND "))
        }
    }
}

fn compile_int_cmp(expr: &str, cmp: &IntComparison, params: &mut Vec<Value>) -> String {
    match cmp {
        IntComparison::Eq { value } => {
            params.push(Value::Integer(*value));
            format!("({expr} = ?)")
        }
        IntComparison::Gte { value } => {
            params.push(Value::Integer(*value));
            format!("({expr} >= ?)")
        }
        IntComparison::Lte { value } => {
            params.push(Value::Integer(*value));
            format!("({expr} <= ?)")
        }
        IntComparison::Between { min, max } => {
            params.push(Value::Integer(*min));
            params.push(Value::Integer(*max));
            format!("({expr} BETWEEN ? AND ?)")
        }
    }
}

fn compile_time_cmp(
    expr: &str,
    cmp: &TimeComparison,
    now_epoch: i64,
    params: &mut Vec<Value>,
) -> String {
    match cmp {
        TimeComparison::WithinLastDays { days } => {
            let cutoff = now_epoch.saturating_sub((*days as i64).saturating_mul(86_400));
            params.push(Value::Integer(cutoff));
            format!("({expr} >= ?)")
        }
        TimeComparison::OlderThanDays { days } => {
            let cutoff = now_epoch.saturating_sub((*days as i64).saturating_mul(86_400));
            params.push(Value::Integer(cutoff));
            format!("({expr} < ?)")
        }
        TimeComparison::AfterEpoch { timestamp } => {
            params.push(Value::Integer(*timestamp));
            format!("({expr} >= ?)")
        }
        TimeComparison::BeforeEpoch { timestamp } => {
            params.push(Value::Integer(*timestamp));
            format!("({expr} <= ?)")
        }
        TimeComparison::BetweenEpochs { min, max } => {
            params.push(Value::Integer(*min));
            params.push(Value::Integer(*max));
            format!("({expr} BETWEEN ? AND ?)")
        }
    }
}

fn compile_condition(
    cond: &FilterCondition,
    user_id: Option<&str>,
    now_epoch: i64,
    state: &mut CompiledFilter,
) -> String {
    match cond {
        FilterCondition::LibraryId { values, .. } => {
            if values.is_empty() {
                return "0 = 1".to_string();
            }
            let ph = values.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            for v in values {
                state.params.push(Value::Text(v.trim().to_string()));
            }
            format!("(m.library_id IN ({ph}))")
        }
        FilterCondition::ItemType { values, .. } => {
            state.references_item_type = true;
            if values.is_empty() {
                return "0 = 1".to_string();
            }
            let ph = values.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            for v in values {
                state
                    .params
                    .push(Value::Text(media_type_db_str(*v).to_string()));
            }
            format!("(m.item_type IN ({ph}))")
        }

        FilterCondition::Title { matcher } => {
            compile_text_match("m.title", matcher, &mut state.params)
        }
        FilterCondition::OriginalTitle { matcher } => {
            compile_text_match("m.original_title", matcher, &mut state.params)
        }
        FilterCondition::SeriesTitle { matcher } => compile_text_match(
            "json_extract(m.metadata, '$.series_title')",
            matcher,
            &mut state.params,
        ),
        FilterCondition::Director { matcher } => compile_text_match(
            "json_extract(m.metadata, '$.director')",
            matcher,
            &mut state.params,
        ),
        FilterCondition::Studio { matcher } => compile_text_match(
            "json_extract(m.metadata, '$.studio')",
            matcher,
            &mut state.params,
        ),
        FilterCondition::Country { matcher } => compile_text_match(
            "json_extract(m.metadata, '$.country')",
            matcher,
            &mut state.params,
        ),
        FilterCondition::Language { matcher } => compile_text_match(
            "json_extract(m.metadata, '$.language')",
            matcher,
            &mut state.params,
        ),
        FilterCondition::Resolution { matcher } => {
            compile_text_match("m.resolution", matcher, &mut state.params)
        }
        FilterCondition::VideoCodec { matcher } => {
            compile_text_match("m.video_codec", matcher, &mut state.params)
        }
        FilterCondition::AudioCodec { matcher } => {
            compile_text_match("m.audio_codec", matcher, &mut state.params)
        }
        FilterCondition::Container { matcher } => {
            compile_text_match("m.container", matcher, &mut state.params)
        }

        FilterCondition::Genres { matcher } => {
            compile_array_match("$.genres", matcher, &mut state.params)
        }
        FilterCondition::Tags { matcher } => {
            compile_array_match("$.tags", matcher, &mut state.params)
        }
        FilterCondition::Actors { matcher } => {
            compile_array_match("$.actors", matcher, &mut state.params)
        }
        FilterCondition::Writers { matcher } => {
            compile_array_match("$.writers", matcher, &mut state.params)
        }

        FilterCondition::ReleaseYear { cmp } => {
            compile_int_cmp("m.release_year", cmp, &mut state.params)
        }
        FilterCondition::DurationSeconds { cmp } => {
            compile_int_cmp("m.duration_seconds", cmp, &mut state.params)
        }
        FilterCondition::AudioChannels { cmp } => {
            compile_int_cmp("m.audio_channels", cmp, &mut state.params)
        }
        FilterCondition::PlayCount { cmp } => {
            state.needs_playback_join = true;
            compile_int_cmp("COALESCE(ups.play_count, 0)", cmp, &mut state.params)
        }
        FilterCondition::Rating { cmp } => {
            let expr = "CAST(json_extract(m.metadata, '$.rating') AS REAL)";
            match cmp {
                FloatComparison::Gte { value } => {
                    state.params.push(Value::Real(f32_to_sql_f64(*value)));
                    format!("({expr} >= ?)")
                }
                FloatComparison::Lte { value } => {
                    state.params.push(Value::Real(f32_to_sql_f64(*value)));
                    format!("({expr} <= ?)")
                }
                FloatComparison::Between { min, max } => {
                    state.params.push(Value::Real(f32_to_sql_f64(*min)));
                    state.params.push(Value::Real(f32_to_sql_f64(*max)));
                    format!("({expr} BETWEEN ? AND ?)")
                }
            }
        }

        FilterCondition::AddedAt { cmp } => {
            compile_time_cmp("m.added_at", cmp, now_epoch, &mut state.params)
        }
        FilterCondition::LastWatchedAt { cmp } => {
            state.needs_playback_join = true;
            compile_time_cmp("ups.last_watched_at", cmp, now_epoch, &mut state.params)
        }

        FilterCondition::WatchState { matcher } => {
            state.needs_playback_join = true;
            match matcher {
                WatchStateMatch::StateIn { values } => {
                    if values.is_empty() {
                        return "0 = 1".to_string();
                    }
                    let ph = values.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
                    for v in values {
                        state
                            .params
                            .push(Value::Text(watch_state_db_str(*v).to_string()));
                    }
                    format!("(COALESCE(ups.watch_state, 'unwatched') IN ({ph}))")
                }
                WatchStateMatch::ProgressBetween { min, max } => {
                    state.params.push(Value::Real(f32_to_sql_f64(*min)));
                    state.params.push(Value::Real(f32_to_sql_f64(*max)));
                    "(m.duration_seconds > 0 AND (CAST(COALESCE(ups.playback_position_seconds, 0) AS REAL) / CAST(m.duration_seconds AS REAL)) BETWEEN ? AND ?)".to_string()
                }
            }
        }

        FilterCondition::IsFavorite { value, .. } => {
            state
                .params
                .push(Value::Text(user_id.unwrap_or("").to_string()));
            if *value {
                "(EXISTS (SELECT 1 FROM user_favorites uf WHERE uf.media_item_id = m.id AND uf.user_id = ?))"
                    .to_string()
            } else {
                "(NOT EXISTS (SELECT 1 FROM user_favorites uf WHERE uf.media_item_id = m.id AND uf.user_id = ?))"
                    .to_string()
            }
        }
    }
}
