ALTER TABLE transcript_segment ADD COLUMN reviewed INTEGER NOT NULL DEFAULT 0;
PRAGMA user_version=5;
