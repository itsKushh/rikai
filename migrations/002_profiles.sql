CREATE TABLE voice_suggestion(meeting_id INTEGER NOT NULL REFERENCES meeting ON DELETE CASCADE,label TEXT NOT NULL,profile_id INTEGER NOT NULL REFERENCES voice_profile ON DELETE CASCADE,score REAL NOT NULL,PRIMARY KEY(meeting_id,label));
PRAGMA user_version=2;
