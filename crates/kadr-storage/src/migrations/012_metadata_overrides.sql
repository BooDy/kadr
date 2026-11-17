-- Add locked_fields JSON array column to protect user and TMDb metadata overrides across rescans.
ALTER TABLE media_items ADD COLUMN locked_fields TEXT NOT NULL DEFAULT '[]';
