-- Add audio_tracks_json column to store probed multi-audio stream metadata per media item.
ALTER TABLE media_items ADD COLUMN audio_tracks_json TEXT;
