CREATE TABLE segment_translation (
    segment_id INTEGER NOT NULL REFERENCES transcript_segment ON DELETE CASCADE,
    language TEXT NOT NULL,
    text TEXT NOT NULL,
    PRIMARY KEY (segment_id, language)
);
CREATE TRIGGER invalidate_translation AFTER UPDATE OF raw, edited ON transcript_segment
WHEN coalesce(NEW.edited, NEW.raw) != coalesce(OLD.edited, OLD.raw)
BEGIN
    DELETE FROM segment_translation WHERE segment_id = NEW.id;
END;
PRAGMA user_version=4;
