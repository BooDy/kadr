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
