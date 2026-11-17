CREATE VIRTUAL TABLE IF NOT EXISTS media_items_fts USING fts5(
    title,
    original_title,
    series_title,
    overview,
    people,
    genres_tags,
    file_name,
    release_year,
    tokenize = 'unicode61 remove_diacritics 2'
);

DELETE FROM media_items_fts;

INSERT INTO media_items_fts (
    rowid,
    title,
    original_title,
    series_title,
    overview,
    people,
    genres_tags,
    file_name,
    release_year
)
SELECT
    m.id,
    COALESCE(m.title, ''),
    COALESCE(m.original_title, ''),
    TRIM(
        COALESCE(
            CASE
                WHEN json_valid(m.metadata) AND json_type(m.metadata, '$.series_title') = 'text'
                THEN json_extract(m.metadata, '$.series_title')
            END,
            ''
        )
        || ' ' ||
        CASE
            WHEN json_valid(m.metadata)
                 AND COALESCE(json_type(m.metadata, '$.season'), json_type(m.metadata, '$.season_number')) = 'integer'
                 AND COALESCE(json_type(m.metadata, '$.episode'), json_type(m.metadata, '$.episode_number')) = 'integer'
            THEN printf(
                'S%02dE%02d',
                COALESCE(json_extract(m.metadata, '$.season'), json_extract(m.metadata, '$.season_number')),
                COALESCE(json_extract(m.metadata, '$.episode'), json_extract(m.metadata, '$.episode_number'))
            )
            ELSE ''
        END
    ),
    COALESCE(
        CASE
            WHEN json_valid(m.metadata) AND json_type(m.metadata, '$.overview') = 'text'
            THEN json_extract(m.metadata, '$.overview')
        END,
        ''
    ),
    TRIM(
        COALESCE(
            CASE
                WHEN json_valid(m.metadata) AND json_type(m.metadata, '$.director') = 'text'
                THEN json_extract(m.metadata, '$.director')
            END,
            ''
        )
        || ' ' ||
        COALESCE(
            (
                SELECT group_concat(value, ' ')
                FROM json_each(
                    CASE
                        WHEN json_valid(m.metadata) AND json_type(m.metadata, '$.writers') = 'array'
                        THEN json_extract(m.metadata, '$.writers')
                        ELSE '[]'
                    END
                )
            ),
            ''
        )
        || ' ' ||
        COALESCE(
            (
                SELECT group_concat(value, ' ')
                FROM json_each(
                    CASE
                        WHEN json_valid(m.metadata) AND json_type(m.metadata, '$.actors') = 'array'
                        THEN json_extract(m.metadata, '$.actors')
                        ELSE '[]'
                    END
                )
            ),
            ''
        )
    ),
    TRIM(
        COALESCE(
            (
                SELECT group_concat(value, ' ')
                FROM json_each(
                    CASE
                        WHEN json_valid(m.metadata) AND json_type(m.metadata, '$.genres') = 'array'
                        THEN json_extract(m.metadata, '$.genres')
                        ELSE '[]'
                    END
                )
            ),
            ''
        )
        || ' ' ||
        COALESCE(
            (
                SELECT group_concat(value, ' ')
                FROM json_each(
                    CASE
                        WHEN json_valid(m.metadata) AND json_type(m.metadata, '$.tags') = 'array'
                        THEN json_extract(m.metadata, '$.tags')
                        ELSE '[]'
                    END
                )
            ),
            ''
        )
        || ' ' ||
        COALESCE(
            CASE
                WHEN json_valid(m.metadata) AND json_type(m.metadata, '$.studio') = 'text'
                THEN json_extract(m.metadata, '$.studio')
            END,
            ''
        )
        || ' ' ||
        COALESCE(
            CASE
                WHEN json_valid(m.metadata) AND json_type(m.metadata, '$.country') = 'text'
                THEN json_extract(m.metadata, '$.country')
            END,
            ''
        )
    ),
    COALESCE(m.file_name, ''),
    COALESCE(CAST(m.release_year AS TEXT), '')
FROM media_items m;

CREATE TRIGGER IF NOT EXISTS trg_media_items_fts_ai
AFTER INSERT ON media_items
BEGIN
    INSERT INTO media_items_fts (
        rowid,
        title,
        original_title,
        series_title,
        overview,
        people,
        genres_tags,
        file_name,
        release_year
    )
    VALUES (
        NEW.id,
        COALESCE(NEW.title, ''),
        COALESCE(NEW.original_title, ''),
        TRIM(
            COALESCE(
                CASE
                    WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.series_title') = 'text'
                    THEN json_extract(NEW.metadata, '$.series_title')
                END,
                ''
            )
            || ' ' ||
            CASE
                WHEN json_valid(NEW.metadata)
                     AND COALESCE(json_type(NEW.metadata, '$.season'), json_type(NEW.metadata, '$.season_number')) = 'integer'
                     AND COALESCE(json_type(NEW.metadata, '$.episode'), json_type(NEW.metadata, '$.episode_number')) = 'integer'
                THEN printf(
                    'S%02dE%02d',
                    COALESCE(json_extract(NEW.metadata, '$.season'), json_extract(NEW.metadata, '$.season_number')),
                    COALESCE(json_extract(NEW.metadata, '$.episode'), json_extract(NEW.metadata, '$.episode_number'))
                )
                ELSE ''
            END
        ),
        COALESCE(
            CASE
                WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.overview') = 'text'
                THEN json_extract(NEW.metadata, '$.overview')
            END,
            ''
        ),
        TRIM(
            COALESCE(
                CASE
                    WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.director') = 'text'
                    THEN json_extract(NEW.metadata, '$.director')
                END,
                ''
            )
            || ' ' ||
            COALESCE(
                (
                    SELECT group_concat(value, ' ')
                    FROM json_each(
                        CASE
                            WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.writers') = 'array'
                            THEN json_extract(NEW.metadata, '$.writers')
                            ELSE '[]'
                        END
                    )
                ),
                ''
            )
            || ' ' ||
            COALESCE(
                (
                    SELECT group_concat(value, ' ')
                    FROM json_each(
                        CASE
                            WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.actors') = 'array'
                            THEN json_extract(NEW.metadata, '$.actors')
                            ELSE '[]'
                        END
                    )
                ),
                ''
            )
        ),
        TRIM(
            COALESCE(
                (
                    SELECT group_concat(value, ' ')
                    FROM json_each(
                        CASE
                            WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.genres') = 'array'
                            THEN json_extract(NEW.metadata, '$.genres')
                            ELSE '[]'
                        END
                    )
                ),
                ''
            )
            || ' ' ||
            COALESCE(
                (
                    SELECT group_concat(value, ' ')
                    FROM json_each(
                        CASE
                            WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.tags') = 'array'
                            THEN json_extract(NEW.metadata, '$.tags')
                            ELSE '[]'
                        END
                    )
                ),
                ''
            )
            || ' ' ||
            COALESCE(
                CASE
                    WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.studio') = 'text'
                    THEN json_extract(NEW.metadata, '$.studio')
                END,
                ''
            )
            || ' ' ||
            COALESCE(
                CASE
                    WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.country') = 'text'
                    THEN json_extract(NEW.metadata, '$.country')
                END,
                ''
            )
        ),
        COALESCE(NEW.file_name, ''),
        COALESCE(CAST(NEW.release_year AS TEXT), '')
    );
END;

CREATE TRIGGER IF NOT EXISTS trg_media_items_fts_au
AFTER UPDATE ON media_items
BEGIN
    DELETE FROM media_items_fts WHERE rowid = OLD.id;
    INSERT INTO media_items_fts (
        rowid,
        title,
        original_title,
        series_title,
        overview,
        people,
        genres_tags,
        file_name,
        release_year
    )
    VALUES (
        NEW.id,
        COALESCE(NEW.title, ''),
        COALESCE(NEW.original_title, ''),
        TRIM(
            COALESCE(
                CASE
                    WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.series_title') = 'text'
                    THEN json_extract(NEW.metadata, '$.series_title')
                END,
                ''
            )
            || ' ' ||
            CASE
                WHEN json_valid(NEW.metadata)
                     AND COALESCE(json_type(NEW.metadata, '$.season'), json_type(NEW.metadata, '$.season_number')) = 'integer'
                     AND COALESCE(json_type(NEW.metadata, '$.episode'), json_type(NEW.metadata, '$.episode_number')) = 'integer'
                THEN printf(
                    'S%02dE%02d',
                    COALESCE(json_extract(NEW.metadata, '$.season'), json_extract(NEW.metadata, '$.season_number')),
                    COALESCE(json_extract(NEW.metadata, '$.episode'), json_extract(NEW.metadata, '$.episode_number'))
                )
                ELSE ''
            END
        ),
        COALESCE(
            CASE
                WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.overview') = 'text'
                THEN json_extract(NEW.metadata, '$.overview')
            END,
            ''
        ),
        TRIM(
            COALESCE(
                CASE
                    WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.director') = 'text'
                    THEN json_extract(NEW.metadata, '$.director')
                END,
                ''
            )
            || ' ' ||
            COALESCE(
                (
                    SELECT group_concat(value, ' ')
                    FROM json_each(
                        CASE
                            WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.writers') = 'array'
                            THEN json_extract(NEW.metadata, '$.writers')
                            ELSE '[]'
                        END
                    )
                ),
                ''
            )
            || ' ' ||
            COALESCE(
                (
                    SELECT group_concat(value, ' ')
                    FROM json_each(
                        CASE
                            WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.actors') = 'array'
                            THEN json_extract(NEW.metadata, '$.actors')
                            ELSE '[]'
                        END
                    )
                ),
                ''
            )
        ),
        TRIM(
            COALESCE(
                (
                    SELECT group_concat(value, ' ')
                    FROM json_each(
                        CASE
                            WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.genres') = 'array'
                            THEN json_extract(NEW.metadata, '$.genres')
                            ELSE '[]'
                        END
                    )
                ),
                ''
            )
            || ' ' ||
            COALESCE(
                (
                    SELECT group_concat(value, ' ')
                    FROM json_each(
                        CASE
                            WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.tags') = 'array'
                            THEN json_extract(NEW.metadata, '$.tags')
                            ELSE '[]'
                        END
                    )
                ),
                ''
            )
            || ' ' ||
            COALESCE(
                CASE
                    WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.studio') = 'text'
                    THEN json_extract(NEW.metadata, '$.studio')
                END,
                ''
            )
            || ' ' ||
            COALESCE(
                CASE
                    WHEN json_valid(NEW.metadata) AND json_type(NEW.metadata, '$.country') = 'text'
                    THEN json_extract(NEW.metadata, '$.country')
                END,
                ''
            )
        ),
        COALESCE(NEW.file_name, ''),
        COALESCE(CAST(NEW.release_year AS TEXT), '')
    );
END;

CREATE TRIGGER IF NOT EXISTS trg_media_items_fts_ad
AFTER DELETE ON media_items
BEGIN
    DELETE FROM media_items_fts WHERE rowid = OLD.id;
END;
