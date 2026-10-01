-- Fast sorting and filtering by rating stored in JSON metadata
CREATE INDEX IF NOT EXISTS idx_media_rating
ON media_items(CAST(json_extract(metadata, '$.rating') AS REAL) DESC);
