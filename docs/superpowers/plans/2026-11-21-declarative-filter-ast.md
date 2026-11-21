# Declarative Filter AST & Visual Rule Builder Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a recursive `AND / OR / NOT` Declarative Filter AST compiled into parameterized SQLite prepared statements with expression indexes, catalog facet suggestions, stateless live widget previews, and a visual Rule Builder in Layout Studio (`v0.4` Part 1).

**Architecture:** `kadr-core` defines strongly-typed recursive `FilterNode` / `FilterCondition` types with strict bounds validation (`max_depth <= 5`, `1..=32` rules) and a pure-Rust reference evaluator (`FilterNode::matches`). `kadr-storage` adds column/JSON expression indexes (`014_filter_ast_indexes.sql`), compiles `FilterNode` into parameterized SQLite queries wrapped inside a fail-closed private-library envelope, and aggregates visible `CatalogFacets`. `kadr-server` wires `QueryMacro::CustomQuery` + `rule_tree` across `WidgetResolver`, validates screen saves, and exposes `GET /api/v1/catalog/facets` and `POST /api/v1/widgets/preview` consumed by modular `RuleBuilderEditor` and `WidgetLivePreview` React components in `web/`.

**Tech Stack:** Rust (`rustc 1.100+`, `serde`, `rusqlite`, `deadpool-sqlite`, `axum`), SQLite WAL + JSON1 (`json_extract`, `json_each`, expression indexes), React 19, TypeScript, Tailwind CSS, Vitest + Testing Library.

## Global Constraints

- Zero new native C/C++ dependencies; preserve `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` static compilation and `<= 30 MB` idle RSS.
- Zero raw SQL string interpolation of user values: every literal in `filter_compiler.rs` MUST bind via `?` (`rusqlite::types::Value`), and `LIKE` patterns MUST escape `\`, `%`, and `_` with `ESCAPE '\'`.
- Fail-closed private library isolation: locked private libraries (`libraries.is_private = 1` not present in signed `UnlockedLibraries` tokens) MUST NEVER leak items, counts, or facet values, even under root `OR` or `NOT` AST nodes.
- 100% backwards compatibility with existing `data/screens/*.json` files and preset `QueryMacro` variants (`ContinueWatching`, `RecentlyAdded`, `TopRated`, `LibraryItems`, `GenreShelf`, `SpotlightItem`, `ItemDetails`, `PlaylistItems`).
- Rust `1.100.0` Clippy clean: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` must pass with zero warnings.

---

## File Structure

- **Create:** `crates/kadr-core/src/filter_ast.rs` — Recursive `FilterNode`, `LogicalOp`, `FilterCondition`, operator enums, `CatalogFacets`, `FilterValidationError`, `FilterNode::validate()`, and `FilterNode::matches()`.
- **Modify:** `crates/kadr-core/src/ast.rs` — Add `QueryMacro::CustomQuery`, add `rule_tree: Option<FilterNode>` to `WidgetFilterConfig`, adjust `PartialEq` derives.
- **Modify:** `crates/kadr-core/src/lib.rs` — Export `pub mod filter_ast; pub use filter_ast::*;`.
- **Create:** `crates/kadr-core/tests/filter_ast_test.rs` — Serde, validation bounds, and truth-table unit tests.
- **Create:** `crates/kadr-storage/src/migrations/014_filter_ast_indexes.sql` — Column & JSON expression indexes on `media_items`.
- **Modify:** `crates/kadr-storage/src/migrations.rs` — Register `014_filter_ast_indexes.sql` after `013_audio_tracks.sql`.
- **Create:** `crates/kadr-storage/src/repos/filter_compiler.rs` — Parameterized SQLite AST compiler (`CompiledFilter`, `compile_filter_node`).
- **Modify:** `crates/kadr-storage/src/repos/widget_queries.rs` — `build_filter_clauses_with_user`, `find_custom_paginated`, `get_catalog_facets`, expanded sort options (`duration_seconds`, `last_watched_at`), and `user_id` support on preset macro queries.
- **Modify:** `crates/kadr-storage/src/repos/media_item_repo.rs` & `crates/kadr-storage/src/repos/mod.rs` — Expose new `WidgetQueries` methods on `MediaItemRepository`.
- **Create:** `crates/kadr-storage/tests/filter_compiler_test.rs` — SQL-vs-Rust parity tests, `NULL`/`NOT` edge cases, and locked private library zero-leak tests.
- **Modify:** `crates/kadr-server/src/resolver/resolver.rs` — Resolve `QueryMacro::CustomQuery`, evaluate `rule_tree` in `is_item_matching_filters` (including `PlaybackState`), and pass `user_id` to `MediaItemRepository` queries.
- **Modify:** `crates/kadr-server/src/api/screen_routes.rs` — Validate `rule_tree` on `PUT /api/v1/screens/{screen_id}` (`400 Bad Request` on invalid AST).
- **Modify:** `crates/kadr-server/src/api/widget_routes.rs` & `crates/kadr-server/src/api/mod.rs` — Add `GET /api/v1/catalog/facets` and `POST /api/v1/widgets/preview`.
- **Modify:** `docs/CLIENT_GUIDE.md` — Document `custom_query`, `FilterNode` JSON grammar, `/api/v1/catalog/facets`, and `/api/v1/widgets/preview`.
- **Create:** `crates/kadr-server/tests/e2e_filter_ast_test.rs` — End-to-end API, privacy, validation, and `<25 ms` debug performance test suite.
- **Create:** `web/src/types/filterAst.ts` — TypeScript definitions for `FilterNode`, `FilterCondition`, `CatalogFacets`, `WidgetPreviewRequest`, `WidgetPreviewResponse`.
- **Modify:** `web/src/types/index.ts` & `web/src/api/client.ts` — Re-export AST types, add `'custom_query'`, `rule_tree`, `getCatalogFacets`, and `previewWidget`.
- **Create:** `web/src/components/studio/RuleBuilderEditor.tsx` & `web/src/components/studio/RuleBuilderEditor.test.tsx` — 2-level visual group builder + templates + facets + JSON AST mode.
- **Create:** `web/src/components/studio/WidgetLivePreview.tsx` & `web/src/components/studio/WidgetLivePreview.test.tsx` — Debounced live match count, execution latency badge, and mini card preview strip.
- **Modify:** `web/src/components/studio/WidgetConfigModal.tsx`, `web/src/components/studio/LayoutStudio.tsx`, & `web/src/components/studio/LayoutStudio.test.tsx` — Wire `custom_query`, new sorts, `RuleBuilderEditor`, `WidgetLivePreview`, and canvas live preview for unsaved/dirty widgets.

---

### Task 1: Core Filter AST Types, Validation, and Pure-Rust Evaluator (`kadr-core`)

**Files:**
- Create: `crates/kadr-core/src/filter_ast.rs`
- Modify: `crates/kadr-core/src/ast.rs:135-190`
- Modify: `crates/kadr-core/src/lib.rs:1-21`
- Test: `crates/kadr-core/tests/filter_ast_test.rs`

**Interfaces:**
- Consumes: `kadr_core::models::{MediaItem, MediaType, PlaybackState, WatchState}`
- Produces:
  - `kadr_core::filter_ast::{LogicalOp, FilterNode, FilterCondition, SetOp, TextMatch, ArrayMatch, IntComparison, FloatComparison, TimeComparison, WatchStateMatch, CatalogFacets, FilterValidationError}`
  - `FilterNode::validate(&self) -> Result<(), FilterValidationError>`
  - `FilterNode::references_item_type(&self) -> bool`
  - `FilterNode::needs_playback_state(&self) -> bool`
  - `FilterNode::matches(&self, item: &MediaItem, playback: Option<&PlaybackState>, now_epoch: i64) -> bool`
  - `QueryMacro::CustomQuery` and `WidgetFilterConfig::rule_tree: Option<FilterNode>`

- [ ] **Step 1: Write the failing unit tests in `crates/kadr-core/tests/filter_ast_test.rs`**

```rust
use kadr_core::ast::{QueryMacro, WidgetFilterConfig, WidgetQueryBinding};
use kadr_core::filter_ast::{
    ArrayMatch, FilterCondition, FilterNode, FloatComparison, IntComparison, LogicalOp, SetOp,
    TextMatch, TimeComparison, WatchStateMatch,
};
use kadr_core::models::{MediaItem, MediaMetadata, MediaType, PlaybackState, TechnicalInfo, WatchState};
use std::path::PathBuf;

fn sample_egyptian_classic() -> MediaItem {
    MediaItem {
        id: Some(1),
        library_id: "lib-movies".to_string(),
        item_type: MediaType::Movie,
        title: "Cairo Station".to_string(),
        original_title: Some("باب الحديد".to_string()),
        release_year: Some(1958),
        added_at: 1_700_000_000,
        file_path: PathBuf::from("/movies/cairo_station.mkv"),
        file_name: "cairo_station.mkv".to_string(),
        file_size: 4_000_000_000,
        technical: TechnicalInfo {
            duration_seconds: 4620, // 77 min
            resolution: Some("1080p".to_string()),
            video_codec: Some("hevc".to_string()),
            audio_codec: Some("flac".to_string()),
            audio_channels: Some(2),
            container: Some("mkv".to_string()),
            audio_tracks: vec![],
        },
        metadata: MediaMetadata {
            director: Some("Youssef Chahine".to_string()),
            writers: vec!["Abdel Hay Adib".to_string()],
            actors: vec!["Farid Shawqi".to_string(), "Hind Rostom".to_string()],
            country: Some("Egypt".to_string()),
            language: Some("ar".to_string()),
            studio: Some("Gabriel Talhami Films".to_string()),
            rating: Some(8.1),
            genres: vec!["Drama".to_string(), "Crime".to_string()],
            tags: vec!["restored".to_string(), "classic".to_string()],
            ..Default::default()
        },
        locked_fields: vec![],
    }
}

#[test]
fn test_filter_node_json_roundtrip_and_legacy_compat() {
    let legacy_json = r#"{"macro_type":"recently_added","limit":10,"filters":{"exclude_private":true,"max_age_days":30}}"#;
    let legacy_binding: WidgetQueryBinding = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(legacy_binding.macro_type, QueryMacro::RecentlyAdded);
    assert!(legacy_binding.filters.as_ref().unwrap().rule_tree.is_none());

    let custom_json = r#"{
        "macro_type": "custom_query",
        "limit": 20,
        "sort": "release_year:asc",
        "filters": {
            "rule_tree": {
                "kind": "group",
                "op": "and",
                "children": [
                    { "kind": "rule", "condition": { "field": "country", "op": "eq", "value": "Egypt" } },
                    { "kind": "rule", "condition": { "field": "duration_seconds", "op": "lte", "value": 5400 } },
                    { "kind": "rule", "negated": true, "condition": { "field": "genres", "op": "contains_any", "values": ["Musical"] } }
                ]
            }
        }
    }"#;
    let parsed: WidgetQueryBinding = serde_json::from_str(custom_json).unwrap();
    assert_eq!(parsed.macro_type, QueryMacro::CustomQuery);
    let tree = parsed.filters.as_ref().unwrap().rule_tree.as_ref().unwrap();
    assert!(tree.validate().is_ok());

    let reserialized = serde_json::to_string(&parsed).unwrap();
    let reparsed: WidgetQueryBinding = serde_json::from_str(&reserialized).unwrap();
    assert_eq!(parsed, reparsed);
}

#[test]
fn test_validation_rejects_excess_depth_empty_groups_and_bad_ranges() {
    // Empty group rejected
    let empty_group = FilterNode::Group {
        op: LogicalOp::And,
        negated: false,
        children: vec![],
    };
    assert!(empty_group.validate().is_err());

    // Depth 6 rejected (max depth is 5)
    let leaf = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::Country {
            matcher: TextMatch::Eq {
                value: "Egypt".to_string(),
            },
        },
    };
    let mut deep = leaf;
    for _ in 0..6 {
        deep = FilterNode::Group {
            op: LogicalOp::And,
            negated: false,
            children: vec![deep],
        };
    }
    assert!(deep.validate().is_err());

    // Inverted range rejected
    let bad_range = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::ReleaseYear {
            cmp: IntComparison::Between {
                min: 1970,
                max: 1960,
            },
        },
    };
    assert!(bad_range.validate().is_err());

    // Out-of-bounds progress rejected
    let bad_progress = FilterNode::Rule {
        negated: false,
        condition: FilterCondition::WatchState {
            matcher: WatchStateMatch::ProgressBetween { min: 0.2, max: 1.5 },
        },
    };
    assert!(bad_progress.validate().is_err());
}

#[test]
fn test_rust_matches_nested_boolean_null_negation_and_watch_state() {
    let item = sample_egyptian_classic();
    let now = 1_700_086_400; // +1 day

    let tree = FilterNode::Group {
        op: LogicalOp::And,
        negated: false,
        children: vec![
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::Country {
                    matcher: TextMatch::Eq {
                        value: "egypt".to_string(),
                    },
                },
            },
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::Genres {
                    matcher: ArrayMatch::ContainsAll {
                        values: vec!["drama".to_string(), "crime".to_string()],
                    },
                },
            },
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::AddedAt {
                    cmp: TimeComparison::WithinLastDays { days: 7 },
                },
            },
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::WatchState {
                    matcher: WatchStateMatch::ProgressBetween { min: 0.1, max: 0.9 },
                },
            },
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::ItemType {
                    op: SetOp::In,
                    values: vec![MediaType::Movie],
                },
            },
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::Rating {
                    cmp: FloatComparison::Gte { value: 7.5 },
                },
            },
        ],
    };

    assert!(tree.references_item_type());
    assert!(tree.needs_playback_state());
    // Unwatched (None playback) has 0.0 progress -> should not match ProgressBetween(0.1, 0.9)
    assert!(!tree.matches(&item, None, now));

    let in_progress = PlaybackState {
        user_id: "u1".to_string(),
        media_item_id: 1,
        playback_position_seconds: 2310, // 50%
        watch_state: WatchState::InProgress,
        last_watched_at: now - 60,
        play_count: 1,
    };
    assert!(tree.matches(&item, Some(&in_progress), now));

    // Negated text rule on NULL field must evaluate to TRUE ("NULL country is NOT France")
    let mut no_country_item = item.clone();
    no_country_item.metadata.country = None;
    let not_france = FilterNode::Rule {
        negated: true,
        condition: FilterCondition::Country {
            matcher: TextMatch::Eq {
                value: "France".to_string(),
            },
        },
    };
    assert!(not_france.matches(&no_country_item, None, now));
    assert_eq!(WidgetFilterConfig::default().rule_tree, None);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-core --test filter_ast_test`
Expected: FAIL to compile (`unresolved import kadr_core::filter_ast`).

- [ ] **Step 3: Implement `crates/kadr-core/src/filter_ast.rs`, `ast.rs`, and `lib.rs`**

Create `crates/kadr-core/src/filter_ast.rs`:

```rust
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::models::{MediaItem, MediaType, PlaybackState, WatchState};

pub const MAX_FILTER_AST_DEPTH: usize = 5;
pub const MAX_FILTER_AST_RULES: usize = 32;
pub const MAX_FILTER_LIST_LEN: usize = 32;
pub const MAX_FILTER_TEXT_LEN: usize = 200;
pub const MAX_FILTER_DAYS: u32 = 36_500;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FilterValidationError {
    #[error("Filter tree exceeds maximum nesting depth of {MAX_FILTER_AST_DEPTH}")]
    MaxDepthExceeded,
    #[error("Filter tree must contain between 1 and {MAX_FILTER_AST_RULES} rules (found {0})")]
    RuleCountOutOfBounds(usize),
    #[error("Filter group cannot have empty children")]
    EmptyGroup,
    #[error("Filter text value cannot be empty or exceed {MAX_FILTER_TEXT_LEN} characters")]
    InvalidTextValue,
    #[error("Filter value list must contain between 1 and {MAX_FILTER_LIST_LEN} items")]
    InvalidListLength,
    #[error("Invalid numeric or time range: min cannot exceed max")]
    InvalidRange,
    #[error("Invalid float value: must be finite and within bounds")]
    InvalidFloatBounds,
    #[error("Invalid days window: must be between 1 and {MAX_FILTER_DAYS}")]
    InvalidDaysBounds,
    #[error("Duration seconds must be non-negative")]
    NegativeDuration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogicalOp {
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FilterNode {
    Group {
        op: LogicalOp,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        negated: bool,
        children: Vec<FilterNode>,
    },
    Rule {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        negated: bool,
        condition: FilterCondition,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetOp {
    In,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TextMatch {
    Eq { value: String },
    In { values: Vec<String> },
    Contains { value: String },
    StartsWith { value: String },
    IsEmpty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ArrayMatch {
    ContainsAny { values: Vec<String> },
    ContainsAll { values: Vec<String> },
    IsEmpty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum IntComparison {
    Eq { value: i64 },
    Gte { value: i64 },
    Lte { value: i64 },
    Between { min: i64, max: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FloatComparison {
    Gte { value: f32 },
    Lte { value: f32 },
    Between { min: f32, max: f32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TimeComparison {
    WithinLastDays { days: u32 },
    OlderThanDays { days: u32 },
    AfterEpoch { timestamp: i64 },
    BeforeEpoch { timestamp: i64 },
    BetweenEpochs { min: i64, max: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum WatchStateMatch {
    StateIn { values: Vec<WatchState> },
    ProgressBetween { min: f32, max: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "field", rename_all = "snake_case")]
pub enum FilterCondition {
    LibraryId { op: SetOp, values: Vec<String> },
    ItemType { op: SetOp, values: Vec<MediaType> },

    Title { #[serde(flatten)] matcher: TextMatch },
    OriginalTitle { #[serde(flatten)] matcher: TextMatch },
    SeriesTitle { #[serde(flatten)] matcher: TextMatch },
    Director { #[serde(flatten)] matcher: TextMatch },
    Studio { #[serde(flatten)] matcher: TextMatch },
    Country { #[serde(flatten)] matcher: TextMatch },
    Language { #[serde(flatten)] matcher: TextMatch },
    Resolution { #[serde(flatten)] matcher: TextMatch },
    VideoCodec { #[serde(flatten)] matcher: TextMatch },
    AudioCodec { #[serde(flatten)] matcher: TextMatch },
    Container { #[serde(flatten)] matcher: TextMatch },

    Genres { #[serde(flatten)] matcher: ArrayMatch },
    Tags { #[serde(flatten)] matcher: ArrayMatch },
    Actors { #[serde(flatten)] matcher: ArrayMatch },
    Writers { #[serde(flatten)] matcher: ArrayMatch },

    ReleaseYear { #[serde(flatten)] cmp: IntComparison },
    DurationSeconds { #[serde(flatten)] cmp: IntComparison },
    AudioChannels { #[serde(flatten)] cmp: IntComparison },
    PlayCount { #[serde(flatten)] cmp: IntComparison },
    Rating { #[serde(flatten)] cmp: FloatComparison },

    AddedAt { #[serde(flatten)] cmp: TimeComparison },
    LastWatchedAt { #[serde(flatten)] cmp: TimeComparison },

    WatchState { #[serde(flatten)] matcher: WatchStateMatch },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogFacets {
    pub genres: Vec<String>,
    pub countries: Vec<String>,
    pub languages: Vec<String>,
    pub studios: Vec<String>,
    pub directors: Vec<String>,
    pub tags: Vec<String>,
    pub resolutions: Vec<String>,
    pub video_codecs: Vec<String>,
    pub audio_codecs: Vec<String>,
    pub containers: Vec<String>,
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
}

fn validate_text_str(s: &str) -> Result<(), FilterValidationError> {
    let t = s.trim();
    if t.is_empty() || t.chars().count() > MAX_FILTER_TEXT_LEN {
        return Err(FilterValidationError::InvalidTextValue);
    }
    Ok(())
}

fn validate_string_list(values: &[String]) -> Result<(), FilterValidationError> {
    if values.is_empty() || values.len() > MAX_FILTER_LIST_LEN {
        return Err(FilterValidationError::InvalidListLength);
    }
    for v in values {
        validate_text_str(v)?;
    }
    Ok(())
}

fn validate_text_match(m: &TextMatch) -> Result<(), FilterValidationError> {
    match m {
        TextMatch::Eq { value } | TextMatch::Contains { value } | TextMatch::StartsWith { value } => {
            validate_text_str(value)
        }
        TextMatch::In { values } => validate_string_list(values),
        TextMatch::IsEmpty => Ok(()),
    }
}

fn validate_array_match(m: &ArrayMatch) -> Result<(), FilterValidationError> {
    match m {
        ArrayMatch::ContainsAny { values } | ArrayMatch::ContainsAll { values } => {
            validate_string_list(values)
        }
        ArrayMatch::IsEmpty => Ok(()),
    }
}

fn validate_int_cmp(c: &IntComparison, non_negative: bool) -> Result<(), FilterValidationError> {
    match c {
        IntComparison::Eq { value } | IntComparison::Gte { value } | IntComparison::Lte { value } => {
            if non_negative && *value < 0 {
                return Err(FilterValidationError::NegativeDuration);
            }
            Ok(())
        }
        IntComparison::Between { min, max } => {
            if non_negative && (*min < 0 || *max < 0) {
                return Err(FilterValidationError::NegativeDuration);
            }
            if min > max {
                return Err(FilterValidationError::InvalidRange);
            }
            Ok(())
        }
    }
}

fn validate_time_cmp(c: &TimeComparison) -> Result<(), FilterValidationError> {
    match c {
        TimeComparison::WithinLastDays { days } | TimeComparison::OlderThanDays { days } => {
            if *days == 0 || *days > MAX_FILTER_DAYS {
                return Err(FilterValidationError::InvalidDaysBounds);
            }
            Ok(())
        }
        TimeComparison::AfterEpoch { .. } | TimeComparison::BeforeEpoch { .. } => Ok(()),
        TimeComparison::BetweenEpochs { min, max } => {
            if min > max {
                return Err(FilterValidationError::InvalidRange);
            }
            Ok(())
        }
    }
}

impl FilterCondition {
    pub fn validate(&self) -> Result<(), FilterValidationError> {
        match self {
            FilterCondition::LibraryId { values, .. } => validate_string_list(values),
            FilterCondition::ItemType { values, .. } => {
                if values.is_empty() || values.len() > MAX_FILTER_LIST_LEN {
                    Err(FilterValidationError::InvalidListLength)
                } else {
                    Ok(())
                }
            }
            FilterCondition::Title { matcher }
            | FilterCondition::OriginalTitle { matcher }
            | FilterCondition::SeriesTitle { matcher }
            | FilterCondition::Director { matcher }
            | FilterCondition::Studio { matcher }
            | FilterCondition::Country { matcher }
            | FilterCondition::Language { matcher }
            | FilterCondition::Resolution { matcher }
            | FilterCondition::VideoCodec { matcher }
            | FilterCondition::AudioCodec { matcher }
            | FilterCondition::Container { matcher } => validate_text_match(matcher),

            FilterCondition::Genres { matcher }
            | FilterCondition::Tags { matcher }
            | FilterCondition::Actors { matcher }
            | FilterCondition::Writers { matcher } => validate_array_match(matcher),

            FilterCondition::ReleaseYear { cmp } => validate_int_cmp(cmp, false),
            FilterCondition::DurationSeconds { cmp }
            | FilterCondition::AudioChannels { cmp }
            | FilterCondition::PlayCount { cmp } => validate_int_cmp(cmp, true),

            FilterCondition::Rating { cmp } => match cmp {
                FloatComparison::Gte { value } | FloatComparison::Lte { value } => {
                    if !value.is_finite() || !(0.0..=10.0).contains(value) {
                        Err(FilterValidationError::InvalidFloatBounds)
                    } else {
                        Ok(())
                    }
                }
                FloatComparison::Between { min, max } => {
                    if !min.is_finite()
                        || !max.is_finite()
                        || !(0.0..=10.0).contains(min)
                        || !(0.0..=10.0).contains(max)
                    {
                        Err(FilterValidationError::InvalidFloatBounds)
                    } else if min > max {
                        Err(FilterValidationError::InvalidRange)
                    } else {
                        Ok(())
                    }
                }
            },

            FilterCondition::AddedAt { cmp } | FilterCondition::LastWatchedAt { cmp } => {
                validate_time_cmp(cmp)
            }

            FilterCondition::WatchState { matcher } => match matcher {
                WatchStateMatch::StateIn { values } => {
                    if values.is_empty() || values.len() > MAX_FILTER_LIST_LEN {
                        Err(FilterValidationError::InvalidListLength)
                    } else {
                        Ok(())
                    }
                }
                WatchStateMatch::ProgressBetween { min, max } => {
                    if !min.is_finite()
                        || !max.is_finite()
                        || !(0.0..=1.0).contains(min)
                        || !(0.0..=1.0).contains(max)
                    {
                        Err(FilterValidationError::InvalidFloatBounds)
                    } else if min > max {
                        Err(FilterValidationError::InvalidRange)
                    } else {
                        Ok(())
                    }
                }
            },
        }
    }
}

fn eval_text_match(actual: Option<&str>, matcher: &TextMatch) -> bool {
    match matcher {
        TextMatch::IsEmpty => actual.map(|s| s.trim().is_empty()).unwrap_or(true),
        TextMatch::Eq { value } => actual
            .map(|s| s.eq_ignore_ascii_case(value.trim()) || s.to_lowercase() == value.trim().to_lowercase())
            .unwrap_or(false),
        TextMatch::In { values } => actual
            .map(|s| {
                let s_low = s.to_lowercase();
                values.iter().any(|v| v.trim().to_lowercase() == s_low)
            })
            .unwrap_or(false),
        TextMatch::Contains { value } => actual
            .map(|s| s.to_lowercase().contains(&value.trim().to_lowercase()))
            .unwrap_or(false),
        TextMatch::StartsWith { value } => actual
            .map(|s| s.to_lowercase().starts_with(&value.trim().to_lowercase()))
            .unwrap_or(false),
    }
}

fn eval_array_match(actual: &[String], matcher: &ArrayMatch) -> bool {
    match matcher {
        ArrayMatch::IsEmpty => actual.is_empty(),
        ArrayMatch::ContainsAny { values } => values.iter().any(|needle| {
            let n = needle.trim().to_lowercase();
            actual.iter().any(|elem| elem.trim().to_lowercase() == n)
        }),
        ArrayMatch::ContainsAll { values } => values.iter().all(|needle| {
            let n = needle.trim().to_lowercase();
            actual.iter().any(|elem| elem.trim().to_lowercase() == n)
        }),
    }
}

fn eval_int_cmp(actual: Option<i64>, cmp: &IntComparison) -> bool {
    let Some(v) = actual else {
        return false;
    };
    match cmp {
        IntComparison::Eq { value } => v == *value,
        IntComparison::Gte { value } => v >= *value,
        IntComparison::Lte { value } => v <= *value,
        IntComparison::Between { min, max } => v >= *min && v <= *max,
    }
}

fn eval_time_cmp(actual: Option<i64>, cmp: &TimeComparison, now_epoch: i64) -> bool {
    let Some(ts) = actual else {
        return false;
    };
    match cmp {
        TimeComparison::WithinLastDays { days } => ts >= now_epoch - (*days as i64 * 86_400),
        TimeComparison::OlderThanDays { days } => ts < now_epoch - (*days as i64 * 86_400),
        TimeComparison::AfterEpoch { timestamp } => ts >= *timestamp,
        TimeComparison::BeforeEpoch { timestamp } => ts <= *timestamp,
        TimeComparison::BetweenEpochs { min, max } => ts >= *min && ts <= *max,
    }
}

impl FilterNode {
    pub fn validate(&self) -> Result<(), FilterValidationError> {
        let mut rule_count = 0usize;
        self.validate_inner(0, &mut rule_count)?;
        if rule_count == 0 || rule_count > MAX_FILTER_AST_RULES {
            return Err(FilterValidationError::RuleCountOutOfBounds(rule_count));
        }
        Ok(())
    }

    fn validate_inner(
        &self,
        group_depth: usize,
        rule_count: &mut usize,
    ) -> Result<(), FilterValidationError> {
        match self {
            FilterNode::Group { children, .. } => {
                let next_depth = group_depth + 1;
                if next_depth > MAX_FILTER_AST_DEPTH {
                    return Err(FilterValidationError::MaxDepthExceeded);
                }
                if children.is_empty() {
                    return Err(FilterValidationError::EmptyGroup);
                }
                for child in children {
                    child.validate_inner(next_depth, rule_count)?;
                }
                Ok(())
            }
            FilterNode::Rule { condition, .. } => {
                *rule_count += 1;
                if *rule_count > MAX_FILTER_AST_RULES {
                    return Err(FilterValidationError::RuleCountOutOfBounds(*rule_count));
                }
                condition.validate()
            }
        }
    }

    pub fn references_item_type(&self) -> bool {
        match self {
            FilterNode::Group { children, .. } => children.iter().any(|c| c.references_item_type()),
            FilterNode::Rule { condition, .. } => {
                matches!(condition, FilterCondition::ItemType { .. })
            }
        }
    }

    pub fn needs_playback_state(&self) -> bool {
        match self {
            FilterNode::Group { children, .. } => children.iter().any(|c| c.needs_playback_state()),
            FilterNode::Rule { condition, .. } => matches!(
                condition,
                FilterCondition::PlayCount { .. }
                    | FilterCondition::LastWatchedAt { .. }
                    | FilterCondition::WatchState { .. }
            ),
        }
    }

    pub fn matches(
        &self,
        item: &MediaItem,
        playback: Option<&PlaybackState>,
        now_epoch: i64,
    ) -> bool {
        match self {
            FilterNode::Group {
                op,
                negated,
                children,
            } => {
                let base = match op {
                    LogicalOp::And => children
                        .iter()
                        .all(|c| c.matches(item, playback, now_epoch)),
                    LogicalOp::Or => children
                        .iter()
                        .any(|c| c.matches(item, playback, now_epoch)),
                };
                if *negated { !base } else { base }
            }
            FilterNode::Rule { negated, condition } => {
                let base = eval_condition(condition, item, playback, now_epoch);
                if *negated { !base } else { base }
            }
        }
    }
}

fn eval_condition(
    condition: &FilterCondition,
    item: &MediaItem,
    playback: Option<&PlaybackState>,
    now_epoch: i64,
) -> bool {
    match condition {
        FilterCondition::LibraryId { values, .. } => values.iter().any(|v| v == &item.library_id),
        FilterCondition::ItemType { values, .. } => values.contains(&item.item_type),

        FilterCondition::Title { matcher } => eval_text_match(Some(&item.title), matcher),
        FilterCondition::OriginalTitle { matcher } => {
            eval_text_match(item.original_title.as_deref(), matcher)
        }
        FilterCondition::SeriesTitle { matcher } => {
            eval_text_match(item.metadata.series_title.as_deref(), matcher)
        }
        FilterCondition::Director { matcher } => {
            eval_text_match(item.metadata.director.as_deref(), matcher)
        }
        FilterCondition::Studio { matcher } => {
            eval_text_match(item.metadata.studio.as_deref(), matcher)
        }
        FilterCondition::Country { matcher } => {
            eval_text_match(item.metadata.country.as_deref(), matcher)
        }
        FilterCondition::Language { matcher } => {
            eval_text_match(item.metadata.language.as_deref(), matcher)
        }
        FilterCondition::Resolution { matcher } => {
            eval_text_match(item.technical.resolution.as_deref(), matcher)
        }
        FilterCondition::VideoCodec { matcher } => {
            eval_text_match(item.technical.video_codec.as_deref(), matcher)
        }
        FilterCondition::AudioCodec { matcher } => {
            eval_text_match(item.technical.audio_codec.as_deref(), matcher)
        }
        FilterCondition::Container { matcher } => {
            eval_text_match(item.technical.container.as_deref(), matcher)
        }

        FilterCondition::Genres { matcher } => eval_array_match(&item.metadata.genres, matcher),
        FilterCondition::Tags { matcher } => eval_array_match(&item.metadata.tags, matcher),
        FilterCondition::Actors { matcher } => eval_array_match(&item.metadata.actors, matcher),
        FilterCondition::Writers { matcher } => eval_array_match(&item.metadata.writers, matcher),

        FilterCondition::ReleaseYear { cmp } => {
            eval_int_cmp(item.release_year.map(|y| y as i64), cmp)
        }
        FilterCondition::DurationSeconds { cmp } => {
            eval_int_cmp(Some(item.technical.duration_seconds), cmp)
        }
        FilterCondition::AudioChannels { cmp } => {
            eval_int_cmp(item.technical.audio_channels.map(|c| c as i64), cmp)
        }
        FilterCondition::PlayCount { cmp } => {
            eval_int_cmp(Some(playback.map(|p| p.play_count as i64).unwrap_or(0)), cmp)
        }
        FilterCondition::Rating { cmp } => {
            let Some(r) = item.metadata.rating else {
                return false;
            };
            match cmp {
                FloatComparison::Gte { value } => r >= *value,
                FloatComparison::Lte { value } => r <= *value,
                FloatComparison::Between { min, max } => r >= *min && r <= *max,
            }
        }

        FilterCondition::AddedAt { cmp } => eval_time_cmp(Some(item.added_at), cmp, now_epoch),
        FilterCondition::LastWatchedAt { cmp } => {
            eval_time_cmp(playback.map(|p| p.last_watched_at), cmp, now_epoch)
        }

        FilterCondition::WatchState { matcher } => match matcher {
            WatchStateMatch::StateIn { values } => {
                let effective = playback
                    .map(|p| p.watch_state)
                    .unwrap_or(WatchState::Unwatched);
                values.contains(&effective)
            }
            WatchStateMatch::ProgressBetween { min, max } => {
                if item.technical.duration_seconds <= 0 {
                    return false;
                }
                let pos = playback.map(|p| p.playback_position_seconds).unwrap_or(0);
                let ratio = (pos as f32) / (item.technical.duration_seconds as f32);
                ratio >= *min && ratio <= *max
            }
        },
    }
}
```

Update `crates/kadr-core/src/ast.rs` (add `use crate::filter_ast::FilterNode;`, add `CustomQuery` to `QueryMacro`, add `pub rule_tree: Option<FilterNode>` to `WidgetFilterConfig`, and remove `Eq` from `WidgetFilterConfig` and `WidgetQueryBinding` while keeping `PartialEq`):

```rust
// Near top of crates/kadr-core/src/ast.rs:
use crate::filter_ast::FilterNode;

// In QueryMacro:
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryMacro {
    ContinueWatching,
    RecentlyAdded,
    TopRated,
    LibraryItems { library_id: String },
    GenreShelf { genre: String },
    SpotlightItem { item_id: Option<i64> },
    ItemDetails { item_id: i64 },
    PlaylistItems { playlist_id: String },
    CustomQuery,
}

// In WidgetFilterConfig (derive Debug, Clone, Default, PartialEq, Serialize, Deserialize):
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WidgetFilterConfig {
    #[serde(default)]
    pub exclude_private: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_library_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_genres: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_days: Option<u32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub all_libraries: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_tree: Option<FilterNode>,
}

// In WidgetQueryBinding (derive Debug, Clone, PartialEq, Serialize, Deserialize):
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidgetQueryBinding {
    pub macro_type: QueryMacro,
    #[serde(default = "default_widget_limit")]
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<WidgetFilterConfig>,
}
```

Update `crates/kadr-core/src/lib.rs` to export `filter_ast`:

```rust
pub mod ast;
pub mod error;
pub mod events;
pub mod filter_ast;
pub mod models;
pub mod playlists;
pub mod plugin;
pub mod search;
pub mod subtitles;
pub mod transcoding;
pub mod video_formats;

pub use ast::*;
pub use error::CoreError;
pub use events::*;
pub use filter_ast::*;
pub use models::*;
pub use playlists::*;
pub use plugin::*;
pub use search::*;
pub use subtitles::*;
pub use transcoding::*;
pub use video_formats::*;
```

- [ ] **Step 4: Run tests and clippy on `kadr-core`**

Run: `cargo test -p kadr-core && cargo clippy -p kadr-core --all-targets -- -D warnings`
Expected: PASS with 0 warnings (update any struct literal in `kadr-core` tests that initializes `WidgetFilterConfig` without `..Default::default()`).

- [ ] **Step 5: Commit Task 1**

```bash
git add crates/kadr-core/
git commit -m "feat(core): add recursive FilterNode AST, validation bounds, and Rust evaluator"
```

---

### Task 2: Parameterized SQL Filter Compiler, Migration `014`, and Facets Queries (`kadr-storage`)

**Files:**
- Create: `crates/kadr-storage/src/migrations/014_filter_ast_indexes.sql`
- Modify: `crates/kadr-storage/src/migrations.rs:1-25`
- Create: `crates/kadr-storage/src/repos/filter_compiler.rs`
- Modify: `crates/kadr-storage/src/repos/widget_queries.rs`
- Modify: `crates/kadr-storage/src/repos/media_item_repo.rs:598-708`
- Modify: `crates/kadr-storage/src/repos/mod.rs`
- Test: `crates/kadr-storage/tests/filter_compiler_test.rs`

**Interfaces:**
- Consumes: `kadr_core::filter_ast::*`, `kadr_core::ast::WidgetFilterConfig`
- Produces:
  - `kadr_storage::repos::filter_compiler::{CompiledFilter, compile_filter_node}`
  - `WidgetQueries::find_custom_paginated(&self, library_id: Option<&str>, user_id: &str, limit: u32, offset: u32, sort: Option<&str>, unlocked_ids: &[String], filters: Option<&WidgetFilterConfig>) -> Result<(Vec<MediaItem>, u64)>`
  - `WidgetQueries::get_catalog_facets(&self, library_id: Option<&str>, unlocked_ids: &[String]) -> Result<CatalogFacets>`
  - `MediaItemRepository::find_custom_paginated(...)`, `MediaItemRepository::get_catalog_facets(...)`, and `user_id: Option<&str>` support on `find_recently_added_paginated_for_user`, `find_top_rated_paginated_for_user`, `find_by_genre_paginated_for_user`, `find_by_library_paginated_for_user`, `find_spotlight_candidate_for_user` (preserving existing non-`_for_user` wrappers so existing callers/tests compile without breaking!).

- [ ] **Step 1: Write the failing storage parity & privacy tests in `crates/kadr-storage/tests/filter_compiler_test.rs`**

```rust
use kadr_core::ast::WidgetFilterConfig;
use kadr_core::filter_ast::{
    ArrayMatch, FilterCondition, FilterNode, FloatComparison, IntComparison, LogicalOp, SetOp,
    TextMatch, TimeComparison, WatchStateMatch,
};
use kadr_core::models::{
    Library, MediaItem, MediaMetadata, MediaType, PlaybackState, TechnicalInfo, User, UserRole,
    WatchState,
};
use kadr_storage::db::create_pool;
use kadr_storage::repos::{LibraryRepository, MediaItemRepository, PlaybackRepository, UserRepository};
use std::path::PathBuf;
use tempfile::tempdir;

#[tokio::test]
async fn test_sql_compiler_parity_with_rust_evaluator_and_private_library_isolation() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_filter_ast.db");
    let pool = create_pool(&db_path, 4).unwrap();

    let user_repo = UserRepository::new(pool.clone());
    let lib_repo = LibraryRepository::new(pool.clone());
    let media_repo = MediaItemRepository::new(pool.clone());
    let playback_repo = PlaybackRepository::new(pool.clone());

    user_repo
        .create(&User {
            id: "u1".to_string(),
            username: "viewer".to_string(),
            pin_hash: "hash".to_string(),
            role: UserRole::Admin,
            created_at: 1_700_000_000,
        })
        .await
        .unwrap();

    lib_repo
        .create(&Library {
            id: "pub-movies".to_string(),
            name: "Movies".to_string(),
            path: PathBuf::from("/pub"),
            paths: vec![PathBuf::from("/pub")],
            media_type: MediaType::Movie,
            is_private: false,
            pin_hash: None,
            created_at: 1_700_000_000,
        })
        .await
        .unwrap();

    lib_repo
        .create(&Library {
            id: "priv-vault".to_string(),
            name: "Private Vault".to_string(),
            path: PathBuf::from("/priv"),
            paths: vec![PathBuf::from("/priv")],
            media_type: MediaType::Movie,
            is_private: true,
            pin_hash: Some("secret".to_string()),
            created_at: 1_700_000_000,
        })
        .await
        .unwrap();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let seed_items = vec![
        MediaItem {
            id: None,
            library_id: "pub-movies".to_string(),
            item_type: MediaType::Movie,
            title: "Cairo Station 100%".to_string(),
            original_title: Some("باب الحديد".to_string()),
            release_year: Some(1958),
            added_at: now - 3 * 86_400,
            file_path: PathBuf::from("/pub/cairo.mkv"),
            file_name: "cairo.mkv".to_string(),
            file_size: 1000,
            technical: TechnicalInfo {
                duration_seconds: 4620,
                resolution: Some("1080p".to_string()),
                video_codec: Some("hevc".to_string()),
                audio_codec: Some("flac".to_string()),
                audio_channels: Some(2),
                container: Some("mkv".to_string()),
                audio_tracks: vec![],
            },
            metadata: MediaMetadata {
                director: Some("Youssef Chahine".to_string()),
                country: Some("Egypt".to_string()),
                language: Some("ar".to_string()),
                studio: Some("Studio Misr".to_string()),
                rating: Some(8.2),
                genres: vec!["Drama".to_string(), "Crime".to_string()],
                tags: vec!["restored".to_string()],
                ..Default::default()
            },
            locked_fields: vec![],
        },
        MediaItem {
            id: None,
            library_id: "pub-movies".to_string(),
            item_type: MediaType::Movie,
            title: "Dune Part Two".to_string(),
            original_title: None,
            release_year: Some(2024),
            added_at: now - 60 * 86_400,
            file_path: PathBuf::from("/pub/dune2.mkv"),
            file_name: "dune2.mkv".to_string(),
            file_size: 2000,
            technical: TechnicalInfo {
                duration_seconds: 9960,
                resolution: Some("4k".to_string()),
                video_codec: Some("av1".to_string()),
                audio_codec: Some("eac3".to_string()),
                audio_channels: Some(6),
                container: Some("mkv".to_string()),
                audio_tracks: vec![],
            },
            metadata: MediaMetadata {
                director: Some("Denis Villeneuve".to_string()),
                country: None, // NULL country to test NOT NULL edge cases
                language: Some("en".to_string()),
                rating: Some(8.6),
                genres: vec!["Sci-Fi".to_string()],
                ..Default::default()
            },
            locked_fields: vec![],
        },
        MediaItem {
            id: None,
            library_id: "pub-movies".to_string(),
            item_type: MediaType::Episode,
            title: "Short Sitcom Pilot".to_string(),
            original_title: None,
            release_year: Some(2022),
            added_at: now - 86_400,
            file_path: PathBuf::from("/pub/ep1.mp4"),
            file_name: "ep1.mp4".to_string(),
            file_size: 500,
            technical: TechnicalInfo {
                duration_seconds: 1320,
                resolution: Some("1080p".to_string()),
                video_codec: Some("h264".to_string()),
                audio_codec: Some("aac".to_string()),
                audio_channels: Some(2),
                container: Some("mp4".to_string()),
                audio_tracks: vec![],
            },
            metadata: MediaMetadata::default(),
            locked_fields: vec![],
        },
        MediaItem {
            id: None,
            library_id: "priv-vault".to_string(),
            item_type: MediaType::Movie,
            title: "Secret Archive Film".to_string(),
            original_title: None,
            release_year: Some(1962),
            added_at: now - 86_400,
            file_path: PathBuf::from("/priv/secret.mkv"),
            file_name: "secret.mkv".to_string(),
            file_size: 1000,
            technical: TechnicalInfo {
                duration_seconds: 4000,
                resolution: Some("4k".to_string()),
                video_codec: Some("hevc".to_string()),
                audio_codec: Some("aac".to_string()),
                audio_channels: Some(2),
                container: Some("mkv".to_string()),
                audio_tracks: vec![],
            },
            metadata: MediaMetadata {
                director: Some("Secret Director".to_string()),
                country: Some("Egypt".to_string()),
                genres: vec!["SecretGenre".to_string()],
                tags: vec!["topsecret".to_string()],
                rating: Some(9.5),
                ..Default::default()
            },
            locked_fields: vec![],
        },
    ];

    media_repo.upsert_batch(&seed_items).await.unwrap();
    let pub_items = media_repo.list_all_by_library("pub-movies").await.unwrap();
    let cairo = pub_items.iter().find(|i| i.title.starts_with("Cairo")).unwrap();

    // Mark Cairo Station 50% watched
    playback_repo
        .upsert_state(&PlaybackState {
            user_id: "u1".to_string(),
            media_item_id: cairo.id.unwrap(),
            playback_position_seconds: 2310,
            watch_state: WatchState::InProgress,
            last_watched_at: now - 120,
            play_count: 1,
        })
        .await
        .unwrap();

    // 1. Root OR query attempting to match private item -> MUST return 0 private items when locked!
    let or_tree = FilterNode::Group {
        op: LogicalOp::Or,
        negated: false,
        children: vec![
            FilterNode::Rule {
                negated: false,
                condition: FilterCondition::Country {
                    matcher: TextMatch::Eq {
                        value: "Egypt".to_string(),
                    },
                },
            },
            FilterNode::Rule {
                negated: true,
                condition: FilterCondition::Title {
                    matcher: TextMatch::Eq {
                        value: "Nonexistent".to_string(),
                    },
                },
            },
        ],
    };
    let filters = WidgetFilterConfig {
        rule_tree: Some(or_tree),
        ..Default::default()
    };

    let (locked_res, locked_total) = media_repo
        .find_custom_paginated(None, "u1", 50, 0, Some("duration_seconds:asc"), &[], Some(&filters))
        .await
        .unwrap();
    // Only Cairo + Dune (episode excluded because rule_tree didn't reference item_type; private item excluded!)
    assert_eq!(locked_total, 2);
    assert!(locked_res.iter().all(|i| i.library_id == "pub-movies"));
    assert_eq!(locked_res[0].title, "Cairo Station 100%");

    // Unlocking priv-vault reveals Secret Archive Film
    let unlocked = vec!["priv-vault".to_string()];
    let (unlocked_res, unlocked_total) = media_repo
        .find_custom_paginated(None, "u1", 50, 0, Some("duration_seconds:asc"), &unlocked, Some(&filters))
        .await
        .unwrap();
    assert_eq!(unlocked_total, 3);
    assert_eq!(unlocked_res[0].title, "Secret Archive Film");

    // 2. Facets NEVER leak SecretGenre / Secret Director / topsecret when locked
    let locked_facets = media_repo.get_catalog_facets(None, &[]).await.unwrap();
    assert!(!locked_facets.genres.iter().any(|g| g == "SecretGenre"));
    assert!(!locked_facets.directors.iter().any(|d| d == "Secret Director"));
    assert!(!locked_facets.tags.iter().any(|t| t == "topsecret"));
    assert!(locked_facets.countries.contains(&"Egypt".to_string()));

    let unlocked_facets = media_repo.get_catalog_facets(None, &unlocked).await.unwrap();
    assert!(unlocked_facets.genres.contains(&"SecretGenre".to_string()));

    // 3. Parity test across NULL negation, LIKE '%' escaping, progress_between, and explicit episode item_type
    let parity_trees = vec![
        // NOT (country = Egypt) -> matches Dune (country IS NULL)
        FilterNode::Rule {
            negated: true,
            condition: FilterCondition::Country {
                matcher: TextMatch::Eq {
                    value: "Egypt".to_string(),
                },
            },
        },
        // Literal '%' in title contains ("100%") -> matches ONLY Cairo Station 100%, not Dune!
        FilterNode::Rule {
            negated: false,
            condition: FilterCondition::Title {
                matcher: TextMatch::Contains {
                    value: "100%".to_string(),
                },
            },
        },
        // Explicit Episode item_type rule -> includes episodes!
        FilterNode::Rule {
            negated: false,
            condition: FilterCondition::ItemType {
                op: SetOp::In,
                values: vec![MediaType::Episode],
            },
        },
        // Watch progress between 0.4 and 0.6 -> matches only Cairo Station
        FilterNode::Group {
            op: LogicalOp::And,
            negated: false,
            children: vec![
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::WatchState {
                        matcher: WatchStateMatch::ProgressBetween { min: 0.4, max: 0.6 },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::Genres {
                        matcher: ArrayMatch::ContainsAll {
                            values: vec!["Drama".to_string(), "Crime".to_string()],
                        },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::Rating {
                        cmp: FloatComparison::Between { min: 8.0, max: 9.0 },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::ReleaseYear {
                        cmp: IntComparison::Lte { value: 1960 },
                    },
                },
                FilterNode::Rule {
                    negated: false,
                    condition: FilterCondition::AddedAt {
                        cmp: TimeComparison::WithinLastDays { days: 7 },
                    },
                },
            ],
        },
    ];

    for tree in parity_trees {
        let cfg = WidgetFilterConfig {
            rule_tree: Some(tree.clone()),
            ..Default::default()
        };
        let (sql_items, _) = media_repo
            .find_custom_paginated(None, "u1", 50, 0, None, &[], Some(&cfg))
            .await
            .unwrap();
        let sql_ids: Vec<i64> = sql_items.iter().filter_map(|i| i.id).collect();

        let mut rust_ids = Vec::new();
        for item in &pub_items {
            if !tree.references_item_type() && item.item_type == MediaType::Episode {
                continue;
            }
            let st = playback_repo.get_state("u1", item.id.unwrap()).await.unwrap();
            if tree.matches(item, st.as_ref(), now) {
                rust_ids.push(item.id.unwrap());
            }
        }
        let mut sql_sorted = sql_ids.clone();
        sql_sorted.sort();
        rust_ids.sort();
        assert_eq!(sql_sorted, rust_ids, "Mismatch on tree: {tree:?}");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-storage --test filter_compiler_test`
Expected: FAIL to compile (`no method named find_custom_paginated` / `get_catalog_facets`).

- [ ] **Step 3: Create `014_filter_ast_indexes.sql`, `filter_compiler.rs`, and update `widget_queries.rs` & `media_item_repo.rs`**

Create `crates/kadr-storage/src/migrations/014_filter_ast_indexes.sql`:

```sql
-- Column & JSON expression indexes for fast Declarative Filter AST evaluation
CREATE INDEX IF NOT EXISTS idx_media_release_year
    ON media_items(release_year DESC);

CREATE INDEX IF NOT EXISTS idx_media_duration
    ON media_items(duration_seconds);

CREATE INDEX IF NOT EXISTS idx_media_resolution_lower
    ON media_items(LOWER(resolution));

CREATE INDEX IF NOT EXISTS idx_media_video_codec_lower
    ON media_items(LOWER(video_codec));

CREATE INDEX IF NOT EXISTS idx_media_country_lower
    ON media_items(LOWER(json_extract(metadata, '$.country')));

CREATE INDEX IF NOT EXISTS idx_media_language_lower
    ON media_items(LOWER(json_extract(metadata, '$.language')));

CREATE INDEX IF NOT EXISTS idx_media_director_lower
    ON media_items(LOWER(json_extract(metadata, '$.director')));

CREATE INDEX IF NOT EXISTS idx_media_studio_lower
    ON media_items(LOWER(json_extract(metadata, '$.studio')));
```

Register `M::up(include_str!("migrations/014_filter_ast_indexes.sql")),` in `crates/kadr-storage/src/migrations.rs` right after `013_audio_tracks.sql`.

Create `crates/kadr-storage/src/repos/filter_compiler.rs`:

```rust
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

fn media_type_db_str(mt: MediaType) -> &'static str {
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
    let mut out = CompiledFilter::default();
    out.sql = compile_inner(node, now_epoch, &mut out);
    out
}

fn compile_inner(node: &FilterNode, now_epoch: i64, state: &mut CompiledFilter) -> String {
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
                .map(|c| compile_inner(c, now_epoch, state))
                .collect();
            let grouped = format!("({})", parts.join(joiner));
            if *negated {
                format!("NOT {grouped}")
            } else {
                grouped
            }
        }
        FilterNode::Rule { negated, condition } => {
            let expr = compile_condition(condition, now_epoch, state);
            if *negated {
                format!("NOT ({expr})")
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
            format!("COALESCE((LOWER({expr}) = ?), 0)")
        }
        TextMatch::In { values } => {
            let ph = values.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            for v in values {
                params.push(Value::Text(v.trim().to_lowercase()));
            }
            format!("COALESCE((LOWER({expr}) IN ({ph})), 0)")
        }
        TextMatch::Contains { value } => {
            let escaped = escape_like(&value.trim().to_lowercase());
            params.push(Value::Text(format!("%{escaped}%")));
            format!("COALESCE((LOWER({expr}) LIKE ? ESCAPE '\\'), 0)")
        }
        TextMatch::StartsWith { value } => {
            let escaped = escape_like(&value.trim().to_lowercase());
            params.push(Value::Text(format!("{escaped}%")));
            format!("COALESCE((LOWER({expr}) LIKE ? ESCAPE '\\'), 0)")
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
            let ph = values.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            for v in values {
                params.push(Value::Text(v.trim().to_lowercase()));
            }
            format!(
                "({arr_expr} IS NOT NULL AND EXISTS (SELECT 1 FROM json_each({arr_expr}) WHERE LOWER(TRIM(value)) IN ({ph})))"
            )
        }
        ArrayMatch::ContainsAll { values } => {
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
            format!("COALESCE(({expr} = ?), 0)")
        }
        IntComparison::Gte { value } => {
            params.push(Value::Integer(*value));
            format!("COALESCE(({expr} >= ?), 0)")
        }
        IntComparison::Lte { value } => {
            params.push(Value::Integer(*value));
            format!("COALESCE(({expr} <= ?), 0)")
        }
        IntComparison::Between { min, max } => {
            params.push(Value::Integer(*min));
            params.push(Value::Integer(*max));
            format!("COALESCE(({expr} BETWEEN ? AND ?), 0)")
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
            let cutoff = now_epoch - (*days as i64 * 86_400);
            params.push(Value::Integer(cutoff));
            format!("COALESCE(({expr} >= ?), 0)")
        }
        TimeComparison::OlderThanDays { days } => {
            let cutoff = now_epoch - (*days as i64 * 86_400);
            params.push(Value::Integer(cutoff));
            format!("COALESCE(({expr} < ?), 0)")
        }
        TimeComparison::AfterEpoch { timestamp } => {
            params.push(Value::Integer(*timestamp));
            format!("COALESCE(({expr} >= ?), 0)")
        }
        TimeComparison::BeforeEpoch { timestamp } => {
            params.push(Value::Integer(*timestamp));
            format!("COALESCE(({expr} <= ?), 0)")
        }
        TimeComparison::BetweenEpochs { min, max } => {
            params.push(Value::Integer(*min));
            params.push(Value::Integer(*max));
            format!("COALESCE(({expr} BETWEEN ? AND ?), 0)")
        }
    }
}

fn compile_condition(
    cond: &FilterCondition,
    now_epoch: i64,
    state: &mut CompiledFilter,
) -> String {
    match cond {
        FilterCondition::LibraryId { values, .. } => {
            let ph = values.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            for v in values {
                state.params.push(Value::Text(v.trim().to_string()));
            }
            format!("(m.library_id IN ({ph}))")
        }
        FilterCondition::ItemType { values, .. } => {
            state.references_item_type = true;
            let ph = values.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            for v in values {
                state
                    .params
                    .push(Value::Text(media_type_db_str(*v).to_string()));
            }
            format!("(m.item_type IN ({ph}))")
        }

        FilterCondition::Title { matcher } => compile_text_match("m.title", matcher, &mut state.params),
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
                    state.params.push(Value::Real(*value as f64));
                    format!("COALESCE(({expr} >= ?), 0)")
                }
                FloatComparison::Lte { value } => {
                    state.params.push(Value::Real(*value as f64));
                    format!("COALESCE(({expr} <= ?), 0)")
                }
                FloatComparison::Between { min, max } => {
                    state.params.push(Value::Real(*min as f64));
                    state.params.push(Value::Real(*max as f64));
                    format!("COALESCE(({expr} BETWEEN ? AND ?), 0)")
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
                    let ph = values.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
                    for v in values {
                        state
                            .params
                            .push(Value::Text(watch_state_db_str(*v).to_string()));
                    }
                    format!("(COALESCE(ups.watch_state, 'unwatched') IN ({ph}))")
                }
                WatchStateMatch::ProgressBetween { min, max } => {
                    state.params.push(Value::Real(*min as f64));
                    state.params.push(Value::Real(*max as f64));
                    "COALESCE((m.duration_seconds > 0 AND (CAST(COALESCE(ups.playback_position_seconds, 0) AS REAL) / CAST(m.duration_seconds AS REAL)) BETWEEN ? AND ?), 0)".to_string()
                }
            }
        }
    }
}
```

In `crates/kadr-storage/src/repos/mod.rs`, add `pub mod filter_compiler;`.

In `crates/kadr-storage/src/repos/widget_queries.rs`:
1. Add `build_filter_envelope(filters: Option<&WidgetFilterConfig>, unlocked_ids: &[String], now_epoch: i64) -> CompiledFilter` which combines `privacy_clause`, legacy `exclude_library_ids` / `exclude_genres` / `max_age_days`, AND `filters.and_then(|f| f.rule_tree.as_ref())` compiled via `compile_filter_node`. Keep `build_filter_clauses(...)` calling `build_filter_envelope(...)` for backwards compatibility!
2. Add `resolve_sort_clause(sort: Option<&str>, default_clause: &'static str) -> (&'static str, bool /* needs_playback_join */)` supporting `duration_seconds:asc|desc` (`m.duration_seconds ASC|DESC, m.id ASC|DESC`) and `last_watched_at:desc|asc` (`COALESCE(ups.last_watched_at, 0) DESC|ASC, m.id DESC|ASC`, returning `needs_playback_join = true`) in addition to existing `title`, `release_year`, `added_at`, and `rating` sorts.
3. Add `find_custom_paginated(&self, library_id: Option<&str>, user_id: &str, limit: u32, offset: u32, sort: Option<&str>, unlocked_ids: &[String], filters: Option<&WidgetFilterConfig>) -> Result<(Vec<MediaItem>, u64)>`:
   - Computes effective `lib_id` (`all_libraries` -> `None`, else `filters.library_id` or `library_id`).
   - Builds `envelope = build_filter_envelope(filters, unlocked_ids, now)`.
   - Adds `LEFT JOIN user_playback_states ups ON ups.media_item_id = m.id AND ups.user_id = ?` whenever `envelope.needs_playback_join || sort_needs_playback_join` (binding `user_id` first!).
   - Applies `m.item_type != 'episode'` UNLESS `envelope.references_item_type` is true.
   - Default sort when `sort` is `None` or `""` is `"m.added_at DESC, m.id DESC"`.
   - Executes `COUNT(*)` and paginated `SELECT` with `LIMIT ? OFFSET ?`.
4. Also add `_for_user` variants (`find_recently_added_paginated_for_user`, `find_top_rated_paginated_for_user`, `find_by_genre_paginated_for_user`, `find_by_library_paginated_for_user`, `find_spotlight_candidate_for_user`) taking `user_id: Option<&str>` (defaulting to `""` in the existing non-`_for_user` wrappers) so `LEFT JOIN user_playback_states` also works when `rule_tree` with watch-state rules is attached to preset macros!
5. Add `get_catalog_facets(&self, library_id: Option<&str>, unlocked_ids: &[String]) -> Result<CatalogFacets>`:
   - Uses `(l.is_private = 0 OR l.id IN (...))` (plus `AND m.library_id = ?` if `library_id` is `Some`).
   - Queries top 100 distinct non-empty trimmed values (`ORDER BY COUNT(*) DESC, val ASC LIMIT 100`) for scalar expressions (`json_extract(m.metadata, '$.country')`, `'$.language'`, `'$.studio'`, `'$.director'`, `m.resolution`, `m.video_codec`, `m.audio_codec`, `m.container`), array expressions (`json_each(json_extract(m.metadata, '$.genres'))`, `'$.tags'`), and `MIN(m.release_year), MAX(m.release_year)`.
6. Expose `find_custom_paginated`, `get_catalog_facets`, and the `*_for_user` methods on `MediaItemRepository` in `crates/kadr-storage/src/repos/media_item_repo.rs`.

- [ ] **Step 4: Run all `kadr-storage` tests and clippy**

Run: `cargo test -p kadr-storage && cargo clippy -p kadr-storage --all-targets -- -D warnings`
Expected: PASS with 0 warnings.

- [ ] **Step 5: Commit Task 2**

```bash
git add crates/kadr-storage/
git commit -m "feat(storage): add parameterized SQL filter AST compiler, expression indexes, and facets query"
```

---

### Task 3: Server Resolver, Validation, `/catalog/facets` & `/widgets/preview` Endpoints, and Docs (`kadr-server`)

**Files:**
- Modify: `crates/kadr-server/src/resolver/resolver.rs:126-200,332-700`
- Modify: `crates/kadr-server/src/api/screen_routes.rs:160-215`
- Modify: `crates/kadr-server/src/api/widget_routes.rs:1-172`
- Modify: `crates/kadr-server/src/api/mod.rs:175-180`
- Modify: `docs/CLIENT_GUIDE.md`
- Test: `crates/kadr-server/tests/e2e_filter_ast_test.rs`

**Interfaces:**
- Consumes: `FilterNode::validate()`, `FilterNode::matches()`, `MediaItemRepository::find_custom_paginated`, `MediaItemRepository::get_catalog_facets`, `MediaItemRepository::*_for_user`
- Produces:
  - `GET /api/v1/catalog/facets?library_id=<optional>` -> `200 OK` `Json<CatalogFacets>`
  - `POST /api/v1/widgets/preview` -> `200 OK` `Json<WidgetPreviewResponse>` (`{ items: Vec<CardViewModel>, total_count: u64, execution_ms: u64 }`) or `400 Bad Request`
  - `PUT /api/v1/screens/{screen_id}` -> `400 Bad Request` if any widget's `rule_tree` fails `FilterNode::validate()`

- [ ] **Step 1: Write the failing E2E test in `crates/kadr-server/tests/e2e_filter_ast_test.rs`**

Look at how existing E2E tests (`crates/kadr-server/tests/search_routes_test.rs` or `tests/e2e_widget_ast_test.rs`) construct the test router via `kadr_server::api::create_router(...)`. Write `crates/kadr-server/tests/e2e_filter_ast_test.rs` testing:
1. `GET /api/v1/catalog/facets` returns public facets and omits locked private library facets until unlocked via `X-Kadr-Unlocked`.
2. `POST /api/v1/widgets/preview` with `macro_type: "custom_query"` and nested `AND / OR / NOT` rules returns `{ items, total_count, execution_ms }` matching expected items, returns `403` for non-admin users, and returns `400` when `rule_tree` has empty children or `min > max`.
3. `PUT /api/v1/screens/home` rejects invalid `rule_tree` with `400 Bad Request`, accepts a valid `custom_query` carousel/grid widget, hydrates it on `GET /api/v1/screens/home`, and paginates via `GET /api/v1/widgets/{id}/data`.
4. Seed 1,000 synthetic `MediaItem`s across genres/countries/years/durations and assert `POST /api/v1/widgets/preview` completes with `execution_ms < 50` in debug mode.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p kadr-server --test e2e_filter_ast_test`
Expected: FAIL (`404 Not Found` on `/api/v1/catalog/facets` and `/api/v1/widgets/preview`, and non-exhaustive `match` on `QueryMacro::CustomQuery` in `resolver.rs`).

- [ ] **Step 3: Wire `resolver.rs`, `screen_routes.rs`, `widget_routes.rs`, `api/mod.rs`, and `docs/CLIENT_GUIDE.md`**

1. **`crates/kadr-server/src/resolver/resolver.rs`:**
   - Update `is_item_matching_filters` to accept `user_id: &str` and `playback_override: Option<&PlaybackState>` so when `f.rule_tree` is `Some(ref tree)`:
     - Fetch `playback` lazily from `self.playback_repo.get_state(user_id, id)` only if `playback_override.is_none() && tree.needs_playback_state()`.
     - Evaluate `if !tree.matches(item, playback_ref, now) { return false; }`.
   - Pass `Some(user_id)` to `find_recently_added_paginated_for_user`, `find_top_rated_paginated_for_user`, `find_by_genre_paginated_for_user`, `find_by_library_paginated_for_user`, and `find_spotlight_candidate_for_user`.
   - Add `QueryMacro::CustomQuery` arm to `resolve_widget_data_with_library`:
     ```rust
     QueryMacro::CustomQuery => {
         let (items, total) = self
             .media_repo
             .find_custom_paginated(
                 effective_library_id,
                 user_id,
                 binding.limit,
                 offset,
                 binding.sort.as_deref(),
                 unlocked_ids,
                 binding.filters.as_ref(),
             )
             .await?;

         let count = items.len() as u32;
         let has_more = (offset as u64 + count as u64) < total;
         let cards = self.hydrate_items_to_cards(items, user_id).await;
         let next_cursor = if has_more {
             Some((offset + count).to_string())
         } else {
             None
         };

         Ok((cards, next_cursor, Some(total)))
     }
     ```
2. **`crates/kadr-server/src/api/screen_routes.rs`:**
   - In `save_screen_handler`, before clearing hydrated items and saving, validate every widget's `rule_tree`:
     ```rust
     for widget in &screen.widgets {
         if let Some(tree) = widget
             .binding()
             .and_then(|b| b.filters.as_ref())
             .and_then(|f| f.rule_tree.as_ref())
         {
             if let Err(err) = tree.validate() {
                 return Err((
                     StatusCode::BAD_REQUEST,
                     Json(serde_json::json!({
                         "error": format!("Invalid filter rule tree in widget '{}': {err}", widget.id())
                     })),
                 ));
             }
         }
     }
     ```
3. **`crates/kadr-server/src/api/widget_routes.rs` & `crates/kadr-server/src/api/mod.rs`:**
   - Add `get_catalog_facets` (`GET /api/v1/catalog/facets`):
     ```rust
     #[derive(Debug, Deserialize)]
     pub struct CatalogFacetsQuery {
         pub library_id: Option<String>,
     }

     pub async fn get_catalog_facets(
         _auth_user: AuthUser,
         unlocked: UnlockedLibraries,
         Query(query): Query<CatalogFacetsQuery>,
         Extension(media_repo): Extension<MediaItemRepository>,
     ) -> Result<Json<CatalogFacets>, (StatusCode, Json<serde_json::Value>)> {
         let unlocked_ids = unlocked.to_vec();
         let facets = media_repo
             .get_catalog_facets(query.library_id.as_deref(), &unlocked_ids)
             .await
             .map_err(|err| {
                 tracing::error!("Failed to load catalog facets: {err}");
                 (
                     StatusCode::INTERNAL_SERVER_ERROR,
                     Json(serde_json::json!({ "error": "Failed to load catalog facets" })),
                 )
             })?;
         Ok(Json(facets))
     }
     ```
   - Add `preview_widget` (`POST /api/v1/widgets/preview`):
     ```rust
     #[derive(Debug, Deserialize)]
     pub struct WidgetPreviewRequest {
         pub binding: WidgetQueryBinding,
         pub screen_id: Option<String>,
         pub limit: Option<u32>,
     }

     #[derive(Debug, Serialize, Deserialize)]
     pub struct WidgetPreviewResponse {
         pub items: Vec<CardViewModel>,
         pub total_count: u64,
         pub execution_ms: u64,
     }

     pub async fn preview_widget(
         _admin: RequireAdmin,
         auth_user: AuthUser,
         unlocked: UnlockedLibraries,
         Extension(resolver): Extension<Arc<WidgetResolver>>,
         Json(payload): Json<WidgetPreviewRequest>,
     ) -> Result<Json<WidgetPreviewResponse>, (StatusCode, Json<serde_json::Value>)> {
         if let Some(tree) = payload
             .binding
             .filters
             .as_ref()
             .and_then(|f| f.rule_tree.as_ref())
         {
             if let Err(err) = tree.validate() {
                 return Err((
                     StatusCode::BAD_REQUEST,
                     Json(serde_json::json!({ "error": err.to_string() })),
                 ));
             }
         }

         let mut binding = payload.binding;
         binding.limit = payload.limit.unwrap_or(10).clamp(1, 24);

         let default_library_id = if let Some(ref sid_str) = payload.screen_id {
             let sid: ScreenId = sid_str.parse().unwrap();
             resolver.resolve_library_id_for_screen(&sid).await
         } else {
             None
         };

         let unlocked_ids = unlocked.to_vec();
         let started = std::time::Instant::now();
         let (items, _next_cursor, total_opt) = resolver
             .resolve_widget_data_with_library(
                 &binding,
                 &auth_user.id,
                 0,
                 &unlocked_ids,
                 default_library_id.as_deref(),
             )
             .await
             .map_err(|err| {
                 tracing::error!("Failed to preview widget query: {err}");
                 (
                     StatusCode::INTERNAL_SERVER_ERROR,
                     Json(serde_json::json!({ "error": "Failed to preview widget query" })),
                 )
             })?;
         let execution_ms = started.elapsed().as_millis() as u64;
         let total_count = total_opt.unwrap_or(items.len() as u64);

         Ok(Json(WidgetPreviewResponse {
             items,
             total_count,
             execution_ms,
         }))
     }
     ```
   - Register `.route("/api/v1/catalog/facets", get(widget_routes::get_catalog_facets))` and `.route("/api/v1/widgets/preview", post(widget_routes::preview_widget))` in `crates/kadr-server/src/api/mod.rs`.
4. **`docs/CLIENT_GUIDE.md`:** Document `custom_query`, `FilterNode` / `FilterCondition` JSON schema, `GET /api/v1/catalog/facets`, and `POST /api/v1/widgets/preview` with examples.

- [ ] **Step 4: Run workspace Rust tests and clippy**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: PASS across all workspace crates with 0 warnings.

- [ ] **Step 5: Commit Task 3**

```bash
git add crates/kadr-server/ tests/ docs/CLIENT_GUIDE.md
git commit -m "feat(server): wire CustomQuery AST resolution, facets endpoint, and stateless widget preview API"
```

---

### Task 4: Frontend Filter AST Types, API Client, `RuleBuilderEditor`, and `WidgetLivePreview` (`web/`)

**Files:**
- Create: `web/src/types/filterAst.ts`
- Modify: `web/src/types/index.ts:55-82`
- Modify: `web/src/api/client.ts`
- Create: `web/src/components/studio/RuleBuilderEditor.tsx`
- Create: `web/src/components/studio/WidgetLivePreview.tsx`
- Test: `web/src/components/studio/RuleBuilderEditor.test.tsx`
- Test: `web/src/components/studio/WidgetLivePreview.test.tsx`

**Interfaces:**
- Consumes: `GET /api/v1/catalog/facets`, `POST /api/v1/widgets/preview`
- Produces:
  - `FilterNode`, `FilterCondition`, `LogicalOp`, `CatalogFacets`, `WidgetPreviewRequest`, `WidgetPreviewResponse`, `getFilterTreeDepth(node)` in `web/src/types/filterAst.ts`
  - `api.getCatalogFacets(libraryId?: string)` and `api.previewWidget(req: WidgetPreviewRequest, signal?: AbortSignal)` in `web/src/api/client.ts`
  - `<RuleBuilderEditor value={ruleTree} onChange={setRuleTree} libraries={libraries} facets={facets} />`
  - `<WidgetLivePreview binding={previewBinding} screenId={screenId} />`

- [ ] **Step 1: Write failing component tests in `RuleBuilderEditor.test.tsx` and `WidgetLivePreview.test.tsx`**

Create `web/src/components/studio/RuleBuilderEditor.test.tsx` verifying:
1. Clicking a Quick Template (e.g., *"Short Films (< 90 min)"*) emits a valid `FilterNode` group (`duration_seconds <= 5400` and `item_type in ['movie']`).
2. Adding a rule, toggling `Match ANY (OR)` and `NOT`, selecting `Country` with facet suggestions (`Egypt`), converting duration minutes (`90` min -> `5400` sec), and adding a nested Rule Group emit exact `FilterNode` JSON structures.
3. Switching to `JSON AST` mode allows editing deeper (`depth = 3`) JSON, and attempting to switch back to `Visual` when `depth > 2` keeps `JSON AST` active with a clear notice banner.

Create `web/src/components/studio/WidgetLivePreview.test.tsx` verifying:
1. Debounces `api.previewWidget` (`vi.useFakeTimers()`) and renders `"3 matches · 2 ms"` plus returned card titles.
2. Renders inline error message when `api.previewWidget` rejects with a `400` validation error.

- [ ] **Step 2: Run Vitest on the new test files to verify they fail**

Run: `cd web && npx vitest run src/components/studio/RuleBuilderEditor.test.tsx src/components/studio/WidgetLivePreview.test.tsx`
Expected: FAIL (`Cannot find module './RuleBuilderEditor'`).

- [ ] **Step 3: Implement `filterAst.ts`, `client.ts` methods, `RuleBuilderEditor.tsx`, and `WidgetLivePreview.tsx`**

1. **`web/src/types/filterAst.ts` & `web/src/types/index.ts`:**
   - Define discriminated union types matching Rust's serde representation (`FilterNode`, `LogicalOp`, `FilterCondition`, `CatalogFacets`, `WidgetPreviewRequest`, `WidgetPreviewResponse`) plus helper `getFilterTreeDepth(node: FilterNode): number`.
   - In `web/src/types/index.ts`, add `'custom_query'` to `QueryMacro`, add `rule_tree?: FilterNode;` to `WidgetFilterConfig`, and `export * from './filterAst';`.
2. **`web/src/api/client.ts`:**
   - Add:
     ```ts
     public async getCatalogFacets(libraryId?: string): Promise<CatalogFacets> {
       const qs = libraryId ? `?library_id=${encodeURIComponent(libraryId)}` : '';
       return this.request<CatalogFacets>(`/api/v1/catalog/facets${qs}`);
     }

     public async previewWidget(
       payload: WidgetPreviewRequest,
       signal?: AbortSignal
     ): Promise<WidgetPreviewResponse> {
       return this.request<WidgetPreviewResponse>('/api/v1/widgets/preview', {
         method: 'POST',
         body: JSON.stringify(payload),
         signal,
       });
     }
     ```
3. **`web/src/components/studio/RuleBuilderEditor.tsx`:**
   - Follow Kadr's dark cinema palette (`theme.md`: `#141417` / `zinc-900` surfaces, `focus-visible:ring-3 focus-visible:ring-amber-400` 10-foot focus rings).
   - Provide `Visual` | `JSON AST` mode tabs, Quick Templates menu, root `ALL (AND)` / `ANY (OR)` + `NOT` controls, `RuleRow` (grouped field `<select>`, context-sensitive operator `<select>`, facet `<datalist>` / multi-chip input, minutes <-> seconds conversion for `duration_seconds`, `0–100%` <-> `0.0–1.0` conversion for `watch_state` `progress_between`), and Level-2 nested `GroupCard`s (`+ Add Rule`, `+ Add Rule Group`, `Clear Rules`).
4. **`web/src/components/studio/WidgetLivePreview.tsx`:**
   - Use `useEffect` with `250ms` `setTimeout` + `AbortController` calling `api.previewWidget?.(...)` (guarding `typeof api.previewWidget === 'function'` so legacy test mocks without `previewWidget` never crash!).
   - Render match count badge (`{total_count} matches · {execution_ms} ms`), empty state hint, validation error alert, and mini horizontal poster/title preview strip.

- [ ] **Step 4: Run Vitest on the new components**

Run: `cd web && npx vitest run src/components/studio/RuleBuilderEditor.test.tsx src/components/studio/WidgetLivePreview.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit Task 4**

```bash
git add web/src/types/ web/src/api/client.ts web/src/components/studio/RuleBuilderEditor* web/src/components/studio/WidgetLivePreview*
git commit -m "feat(web): add Filter AST types, RuleBuilderEditor, and WidgetLivePreview studio components"
```

---

### Task 5: `WidgetConfigModal` & `LayoutStudio` Canvas Integration (`web/`)

**Files:**
- Modify: `web/src/components/studio/WidgetConfigModal.tsx`
- Modify: `web/src/components/studio/LayoutStudio.tsx`
- Modify: `web/src/components/studio/LayoutStudio.test.tsx`

**Interfaces:**
- Consumes: `RuleBuilderEditor`, `WidgetLivePreview`, `api.getCatalogFacets`, `api.previewWidget`
- Produces: Complete end-to-end Layout Studio Rule Builder workflow (`Custom Filter Query` shelf creation, rule-tree refinement on any macro, `duration_seconds` & `last_watched_at` sorts, and live canvas preview of unsaved/dirty widgets).

- [ ] **Step 1: Add failing integration tests in `web/src/components/studio/LayoutStudio.test.tsx`**

Add tests in `web/src/components/studio/LayoutStudio.test.tsx` verifying:
1. Opening `Add Widget` -> selecting `Custom Filter Query` (`custom_query`) -> applying a Quick Template or adding a rule (`Country = Egypt`) -> saving emits a widget with `macro_type: 'custom_query'` and `filters.rule_tree` populated, and renders live preview items on the Studio canvas via `api.previewWidget` before clicking `Save Layout`.
2. Editing an existing widget with `filters.rule_tree` loads its rules in `WidgetConfigModal` and preserves/updates `rule_tree` on save.

- [ ] **Step 2: Run Vitest on `LayoutStudio.test.tsx` to verify the new tests fail**

Run: `cd web && npx vitest run src/components/studio/LayoutStudio.test.tsx`
Expected: FAIL (`custom_query` option and `RuleBuilderEditor` not yet wired in `WidgetConfigModal`).

- [ ] **Step 3: Wire `WidgetConfigModal.tsx` and `LayoutStudio.tsx`**

1. **`web/src/components/studio/WidgetConfigModal.tsx`:**
   - Add `'custom_query'` (`Custom Filter Query`) to `MacroTypeOption` and the Data Source `<select>`.
   - Add `{ value: 'duration_seconds:asc', label: 'Duration (Shortest)' }`, `{ value: 'duration_seconds:desc', label: 'Duration (Longest)' }`, and `{ value: 'last_watched_at:desc', label: 'Last Watched (Recent)' }` to `SORT_OPTIONS`.
   - Add `ruleTree: FilterNode | null` state (initialized from `binding.filters?.rule_tree ?? null`; when user switches `macroType` to `'custom_query'` and `ruleTree` is `null`, initialize a starter 1-rule `Group` or allow empty `rule_tree` matching all visible non-episode items).
   - Fetch `api.getCatalogFacets?.()` once when modal opens (with defensive `typeof api.getCatalogFacets === 'function'` fallback to empty facets so existing tests without `getCatalogFacets` mocks remain 100% green).
   - Render `<RuleBuilderEditor />` inside the Filters panel (auto-expanded when `macroType === 'custom_query'` or `ruleTree !== null`), include `ruleTree ? 1 : 0` in `activeFilterCount`, attach `filterConfig.rule_tree = ruleTree` on submit when `ruleTree` has at least 1 child rule, and render `<WidgetLivePreview binding={draftBinding} screenId={screenId} />` above the modal footer.
2. **`web/src/components/studio/LayoutStudio.tsx`:**
   - Pass `screenId={selectedScreenId}` into `<WidgetConfigModal />`.
   - In `HydratedSpotlight`, `StudioCarousel`, and `StudioGrid` (accepting `screenId: string` and `isDirty: boolean` props): when `widget.items` / `widget.data` is absent (such as right after creating or editing a widget in Studio before saving to disk!), prefer `api.previewWidget?.({ binding: widget.binding, screen_id: screenId, limit })` (falling back to `api.getWidgetData` if `previewWidget` is unavailable in unit-test mocks) so unsaved `custom_query` and rule-filtered shelves display real live catalog items immediately on the Studio canvas!

- [ ] **Step 4: Run full frontend test suite, production build, and full Rust workspace suite**

Run:
```bash
cd web && npm test -- --run && npm run build && cd ..
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```
Expected: 100% of Vitest suites pass, `tsc -b && vite build` succeeds cleanly, and 100% of Rust workspace tests & clippy pass with 0 warnings.

- [ ] **Step 5: Commit Task 5**

```bash
git add web/src/components/studio/
git commit -m "feat(studio): integrate Custom Filter Query, RuleBuilderEditor, and live canvas preview"
```
