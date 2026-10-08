CREATE TABLE IF NOT EXISTS authorization_role_graphs (
  namespace TEXT PRIMARY KEY,
  revision TEXT NOT NULL,
  graph TEXT NOT NULL
);
