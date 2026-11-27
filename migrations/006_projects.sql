CREATE TABLE project(id INTEGER PRIMARY KEY, name TEXT NOT NULL COLLATE NOCASE UNIQUE CHECK(length(trim(name))>0));
CREATE TABLE project_item(project_id INTEGER NOT NULL REFERENCES project ON DELETE CASCADE, meeting_id INTEGER NOT NULL REFERENCES meeting ON DELETE CASCADE, PRIMARY KEY(project_id,meeting_id));
CREATE INDEX project_item_record ON project_item(meeting_id);
INSERT OR IGNORE INTO project(name) SELECT DISTINCT trim(project) FROM meeting WHERE trim(project)<>'';
INSERT INTO project_item SELECT p.id,m.id FROM project p JOIN meeting m ON p.name=trim(m.project);
CREATE TRIGGER project_record_insert AFTER INSERT ON meeting WHEN trim(NEW.project)<>'' BEGIN
 INSERT OR IGNORE INTO project(name) VALUES(trim(NEW.project));
 INSERT OR IGNORE INTO project_item SELECT id,NEW.id FROM project WHERE name=trim(NEW.project);
END;
CREATE TRIGGER project_record_change AFTER UPDATE OF project ON meeting WHEN NEW.project<>OLD.project BEGIN
 DELETE FROM project_item WHERE meeting_id=NEW.id AND project_id=(SELECT id FROM project WHERE name=trim(OLD.project));
 INSERT OR IGNORE INTO project(name) SELECT trim(NEW.project) WHERE trim(NEW.project)<>'';
 INSERT OR IGNORE INTO project_item SELECT id,NEW.id FROM project WHERE name=trim(NEW.project);
END;
CREATE TABLE marked_clip(meeting_id INTEGER NOT NULL REFERENCES meeting ON DELETE CASCADE, start_ms INTEGER NOT NULL CHECK(start_ms>=0),end_ms INTEGER NOT NULL CHECK(end_ms>start_ms),PRIMARY KEY(meeting_id,start_ms,end_ms));
PRAGMA user_version=6;
