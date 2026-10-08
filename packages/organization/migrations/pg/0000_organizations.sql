CREATE TABLE organizations (
  id text PRIMARY KEY,
  version bigint NOT NULL CHECK (version BETWEEN 1 AND 9007199254740991),
  state text NOT NULL CHECK (jsonb_typeof(state::jsonb) = 'object')
);
