CREATE TABLE meeting(
 id INTEGER PRIMARY KEY, title TEXT NOT NULL, project TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '',
 created_at INTEGER NOT NULL DEFAULT(unixepoch()), started_at INTEGER NOT NULL DEFAULT(unixepoch()), ended_at INTEGER,
 duration_ms INTEGER NOT NULL DEFAULT 0, status TEXT NOT NULL, options TEXT NOT NULL DEFAULT '{}');
CREATE TABLE audio_source(id INTEGER PRIMARY KEY, meeting_id INTEGER NOT NULL REFERENCES meeting ON DELETE CASCADE, name TEXT NOT NULL, identity_json TEXT NOT NULL);
CREATE TABLE audio_track(id INTEGER PRIMARY KEY, source_id INTEGER REFERENCES audio_source ON DELETE CASCADE, meeting_id INTEGER NOT NULL REFERENCES meeting ON DELETE CASCADE, file TEXT NOT NULL, UNIQUE(meeting_id,file));
CREATE TABLE transcript(id INTEGER PRIMARY KEY, meeting_id INTEGER NOT NULL REFERENCES meeting ON DELETE CASCADE, settings TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT(unixepoch()));
CREATE TABLE transcript_segment(id INTEGER PRIMARY KEY, transcript_id INTEGER NOT NULL REFERENCES transcript ON DELETE CASCADE, track TEXT NOT NULL, start_ms INTEGER NOT NULL,end_ms INTEGER NOT NULL CHECK(end_ms>=start_ms), speaker TEXT NOT NULL, raw TEXT NOT NULL, edited TEXT, words TEXT NOT NULL, confidence REAL);
CREATE TABLE speaker_turn(id INTEGER PRIMARY KEY, transcript_id INTEGER NOT NULL REFERENCES transcript ON DELETE CASCADE, start_ms INTEGER NOT NULL,end_ms INTEGER NOT NULL,label TEXT NOT NULL);
CREATE TABLE voice_profile(id INTEGER PRIMARY KEY, name TEXT NOT NULL, model TEXT NOT NULL, embedding BLOB NOT NULL, samples TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT(unixepoch()), updated_at INTEGER NOT NULL DEFAULT(unixepoch()));
CREATE TABLE meeting_speaker_assignment(meeting_id INTEGER NOT NULL REFERENCES meeting ON DELETE CASCADE, label TEXT NOT NULL, profile_id INTEGER REFERENCES voice_profile ON DELETE SET NULL, display_name TEXT, PRIMARY KEY(meeting_id,label));
CREATE TABLE summary(id INTEGER PRIMARY KEY, meeting_id INTEGER NOT NULL REFERENCES meeting ON DELETE CASCADE, transcript_id INTEGER REFERENCES transcript, body TEXT NOT NULL, settings TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT(unixepoch()));
CREATE TABLE summary_evidence(summary_id INTEGER NOT NULL REFERENCES summary ON DELETE CASCADE, segment_id INTEGER NOT NULL REFERENCES transcript_segment, PRIMARY KEY(summary_id,segment_id));
CREATE TABLE processing_job(id INTEGER PRIMARY KEY, meeting_id INTEGER NOT NULL REFERENCES meeting ON DELETE CASCADE, stage TEXT NOT NULL, status TEXT NOT NULL, error TEXT);
CREATE TABLE meeting_tag(meeting_id INTEGER NOT NULL REFERENCES meeting ON DELETE CASCADE, tag TEXT NOT NULL, PRIMARY KEY(meeting_id,tag));
CREATE TABLE preference(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE INDEX transcript_versions ON transcript(meeting_id,id);
CREATE INDEX segment_time ON transcript_segment(transcript_id,start_ms);
CREATE VIRTUAL TABLE title_search USING fts5(title, tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER title_insert AFTER INSERT ON meeting BEGIN INSERT INTO title_search(rowid,title) VALUES(new.id,new.title); END;
CREATE TRIGGER title_update AFTER UPDATE OF title ON meeting BEGIN UPDATE title_search SET title=new.title WHERE rowid=new.id; END;
CREATE TRIGGER title_delete AFTER DELETE ON meeting BEGIN DELETE FROM title_search WHERE rowid=old.id; END;
CREATE VIRTUAL TABLE speech_search USING fts5(text,speaker,tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER speech_insert AFTER INSERT ON transcript_segment BEGIN INSERT INTO speech_search(rowid,text,speaker) VALUES(new.id,coalesce(new.edited,new.raw),new.speaker); END;
CREATE TRIGGER speech_update AFTER UPDATE OF edited,speaker ON transcript_segment BEGIN UPDATE speech_search SET text=coalesce(new.edited,new.raw),speaker=new.speaker WHERE rowid=new.id; END;
CREATE TRIGGER speech_delete AFTER DELETE ON transcript_segment BEGIN DELETE FROM speech_search WHERE rowid=old.id; END;
PRAGMA user_version=1;
