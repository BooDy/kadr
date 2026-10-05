# Widget Advanced Filters Design Specification

## Overview
This specification details the architecture, data models, database queries, and user interface for applying advanced filters to screen widgets in Kadr. Filters allow screen layouts to selectively exclude items from private libraries (even when unlocked), exclude specific libraries, exclude specific genres, and exclude items older than a specified duration.

---

## 1. Background & Motivation
* **Problem**:
  * Currently, widget query macros (e.g. `recently_added`, `top_rated`, `continue_watching`, `genre_shelf`, `library_items`) query all accessible media items without granular exclusion options.
  * Users designing custom home or room screens (e.g., Kids screens, Living Room TV, specific genre hubs) cannot prevent certain libraries (e.g. Test, Personal, or Unlocked Private libraries) or genres (e.g., Horror) from appearing in general shelves.
  * Users also cannot limit shelves to recent additions (e.g., "Recently Added in the Last 30 Days").
* **Goals**:
  * Extend `WidgetQueryBinding` with an optional `filters: Option<WidgetFilterConfig>`.
  * Push filtering directly down into SQLite SQL queries for exact pagination and performance.
  * Provide an intuitive "Advanced Filters" configuration UI inside `WidgetConfigModal.tsx` in Layout Studio.
  * Maintain 100% backward compatibility with existing screen layouts.

---

## 2. Architecture & Data Flow

```mermaid
flowchart TD
    subgraph UI ["Layout Studio & Web Client"]
        Modal["WidgetConfigModal<br/>Advanced Filters Form"]
        Studio["LayoutStudio<br/>Save Screen AST"]
        Browse["BrowseScreen / Widgets<br/>GET /api/v1/screens/:id or widget data"]
    end

    subgraph Backend ["Server & Resolver (kadr-server)"]
        AST["ScreenLayout AST<br/>WidgetNode.binding.filters"]
        Resolver["WidgetResolver<br/>resolve_widget_data_with_unlocked"]
    end

    subgraph Storage ["SQLite Storage (kadr-storage)"]
        Builder["build_filter_clauses<br/>(Exclude Private, Exclude Libs, Exclude Genres, Date Cutoff)"]
        SQL["Indexed SQLite Query<br/>WHERE ... AND m.added_at >= ? ..."]
    end

    Modal -->|Define filters| Studio
    Studio -->|PUT /api/v1/screens/:id| AST
    Browse -->|Fetch widget data| Resolver
    AST --> Resolver
    Resolver --> Builder
    Builder --> SQL
    SQL -->|Exact count & paginated items| Resolver
    Resolver -->|Hydrated CardViewModel[]| Browse
```

---

## 3. Detailed Specifications

### 3.1 Core AST Schema (`crates/kadr-core`)

#### `WidgetFilterConfig`
In `crates/kadr-core/src/ast.rs`:
```rust
/// Configuration for filtering items returned by a widget query.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WidgetFilterConfig {
    /// When true, always excludes items from private libraries, even if unlocked.
    #[serde(default)]
    pub exclude_private: bool,

    /// Library IDs to exclude from results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_library_ids: Vec<String>,

    /// Genres to exclude from results (case-insensitive matching).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_genres: Vec<String>,

    /// Maximum age in days from current time; excludes items added before (now - max_age_days).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_days: Option<u32>,
}
```

#### `WidgetQueryBinding` Update
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WidgetQueryBinding {
    pub macro_type: QueryMacro,
    #[serde(default = "default_widget_limit")]
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<WidgetFilterConfig>,
}

impl WidgetQueryBinding {
    pub fn with_filters(mut self, filters: WidgetFilterConfig) -> Self {
        self.filters = Some(filters);
        self
    }
}
```

---

### 3.2 SQL Push-Down Querying (`crates/kadr-storage`)

In `crates/kadr-storage/src/repos/widget_queries.rs`:

#### Clause Builder
```rust
pub fn build_filter_clauses(
    filters: Option<&WidgetFilterConfig>,
    unlocked_ids: &[String],
    current_time_epoch_secs: i64,
) -> (String, Vec<rusqlite::types::Value>) {
    let mut clauses = Vec::new();
    let mut params = Vec::new();

    // 1. Privacy filter
    if filters.map(|f| f.exclude_private).unwrap_or(false) {
        clauses.push("(l.is_private = 0)".to_string());
    } else {
        let (priv_clause, priv_params) = privacy_clause(unlocked_ids);
        clauses.push(priv_clause);
        params.extend(priv_params);
    }

    if let Some(f) = filters {
        // 2. Exclude specific library IDs
        if !f.exclude_library_ids.is_empty() {
            let placeholders = f.exclude_library_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            clauses.push(format!("m.library_id NOT IN ({placeholders})"));
            for lib_id in &f.exclude_library_ids {
                params.push(rusqlite::types::Value::Text(lib_id.clone()));
            }
        }

        // 3. Exclude specific genres
        if !f.exclude_genres.is_empty() {
            let placeholders = f.exclude_genres.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            clauses.push(format!(
                "NOT EXISTS (
                    SELECT 1 FROM json_each(json_extract(m.metadata, '$.genres'))
                    WHERE LOWER(value) IN ({placeholders})
                )"
            ));
            for genre in &f.exclude_genres {
                params.push(rusqlite::types::Value::Text(genre.trim().to_lowercase()));
            }
        }

        // 4. Exclude by date added (max age cutoff)
        if let Some(days) = f.max_age_days {
            if days > 0 {
                let cutoff = current_time_epoch_secs - (days as i64 * 86_400);
                clauses.push("m.added_at >= ?".to_string());
                params.push(rusqlite::types::Value::Integer(cutoff));
            }
        }
    }

    (clauses.join(" AND "), params)
}
```

#### Integration across `WidgetQueries`
Wire `filters: Option<&WidgetFilterConfig>` into:
- `find_recently_added_paginated`
- `find_top_rated_paginated`
- `find_by_genre_paginated`
- `find_by_library_paginated`
- `find_spotlight_candidate`

---

### 3.3 Resolver Integration (`crates/kadr-server`)

In `crates/kadr-server/src/resolver/resolver.rs`:
- Pass `binding.filters.as_ref()` from `WidgetQueryBinding` into the respective `WidgetQueries` calls.
- In `ContinueWatching`: evaluate items against `is_item_matching_filters(item, filters)` before returning to prevent excluded libraries, genres, or private items from appearing.
- In `HeroBanner` / `Spotlight`: apply `binding.filters` when picking or validating candidate items.

---

### 3.4 Web Client & Layout Studio UI (`web/`)

#### 1. TypeScript Types (`web/src/types/index.ts`)
```typescript
export interface WidgetFilterConfig {
  exclude_private?: boolean;
  exclude_library_ids?: string[];
  exclude_genres?: string[];
  max_age_days?: number;
}

export interface WidgetQueryBinding {
  macro_type: QueryMacro | string | Record<string, unknown>;
  limit: number;
  sort?: string;
  filters?: WidgetFilterConfig;
}
```

#### 2. Layout Studio Modal (`web/src/components/studio/WidgetConfigModal.tsx`)
- Collapsible section: `Advanced Filters` with toggle arrow and badge counting active filters.
- **Exclude Private Libraries**: Checkbox with explanatory note.
- **Exclude Specific Libraries**: List of library checkboxes.
- **Exclude Genres**: Text input for comma-delimited genres + quick-toggle buttons for popular genres (`Horror`, `Romance`, `Kids`, `Animation`, `Documentary`, `Drama`).
- **Exclude by Date Added**: Number input for days + quick presets (`7 Days`, `30 Days`, `90 Days`, `1 Year`, `All Time`).
- Adheres to `theme.md` tokens and 10-foot TV UI focus rings (`focus-visible:ring-3 focus-visible:ring-highlight focus-visible:outline-none`).

---

## 4. Verification & Testing Plan

### 4.1 Rust Tests
- `crates/kadr-core/tests/ast_test.rs`: Serde roundtrip tests for `WidgetFilterConfig` and `WidgetQueryBinding` with and without filters.
- `crates/kadr-storage/tests/widget_queries_test.rs`:
  - Test `exclude_private: true` ignores unlocked token.
  - Test `exclude_library_ids` excludes items from matching libraries.
  - Test `exclude_genres` excludes matching genre items.
  - Test `max_age_days` excludes items added before cutoff timestamp.
  - Test combined filters in a single query.
- `crates/kadr-server/tests/widget_resolver_test.rs`:
  - Test `resolve_widget_data` correctly passes filters and returns filtered results across macros.

### 4.2 Frontend Tests
- `web/src/components/studio/WidgetConfigModal.test.tsx`:
  - Verify expanding "Advanced Filters" section.
  - Verify toggling "Exclude Private Libraries".
  - Verify selecting excluded libraries.
  - Verify clicking genre quick-chips and typing custom genres.
  - Verify selecting date added presets (e.g. 30 Days).
  - Verify submitting produces correct `filters` object in `WidgetNode.binding`.
- `web/src/components/studio/LayoutStudio.test.tsx`:
  - Verify creating and updating widgets with filters updates AST.

### 4.3 Workspace Verification
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cd web && npm test -- --run`
- `cd web && npm run build`
