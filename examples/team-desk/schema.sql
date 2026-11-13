PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA busy_timeout = 5000;
CREATE TABLE IF NOT EXISTS desk_members (
  team TEXT NOT NULL, subject TEXT NOT NULL, role TEXT NOT NULL CHECK(role IN ('admin','editor','viewer')),
  PRIMARY KEY(team, subject)
);
CREATE TABLE IF NOT EXISTS desk_projects (
  team TEXT NOT NULL, id TEXT NOT NULL, name TEXT NOT NULL, archived INTEGER NOT NULL DEFAULT 0,
  version INTEGER NOT NULL DEFAULT 1, PRIMARY KEY(team,id)
);
CREATE TABLE IF NOT EXISTS desk_items (
  team TEXT NOT NULL, id TEXT NOT NULL, project TEXT NOT NULL, title TEXT NOT NULL,
  assignee TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('open','doing','done')),
  version INTEGER NOT NULL DEFAULT 1, PRIMARY KEY(team,id),
  FOREIGN KEY(team,project) REFERENCES desk_projects(team,id),
  FOREIGN KEY(team,assignee) REFERENCES desk_members(team,subject)
);
CREATE TABLE IF NOT EXISTS desk_notifications (
  job_id TEXT PRIMARY KEY, team TEXT NOT NULL, recipient TEXT NOT NULL, item TEXT NOT NULL,
  actor TEXT NOT NULL, kind TEXT NOT NULL, version INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS desk_calls (
  id INTEGER PRIMARY KEY, call_id TEXT NOT NULL, action TEXT NOT NULL, subject TEXT, outcome TEXT NOT NULL
);
