CREATE TABLE IF NOT EXISTS authorization_role_graphs (
  namespace text PRIMARY KEY,
  revision text NOT NULL,
  graph jsonb NOT NULL
);
