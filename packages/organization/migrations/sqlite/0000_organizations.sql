CREATE TABLE organizations (
  id TEXT PRIMARY KEY,
  version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 9007199254740991),
  state TEXT NOT NULL CHECK (json_valid(state) AND json_type(state) = 'object')
);
