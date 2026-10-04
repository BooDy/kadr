ALTER TABLE libraries ADD COLUMN is_private INTEGER NOT NULL DEFAULT 0;
ALTER TABLE libraries ADD COLUMN pin_hash TEXT DEFAULT NULL;
CREATE INDEX IF NOT EXISTS idx_libraries_private ON libraries(is_private);
