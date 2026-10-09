-- Host-executed PostgreSQL 12+ migration. Custom table names: postgresSearchMigration().
CREATE TABLE "lenso_search_documents" (
  namespace text NOT NULL,
  tenant_id text NOT NULL DEFAULT '',
  owner_id text NOT NULL DEFAULT '',
  document_type text COLLATE "C" NOT NULL,
  document_id text COLLATE "C" NOT NULL,
  title text NOT NULL,
  body text NOT NULL,
  metadata jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(metadata) = 'object'),
  language regconfig NOT NULL CHECK (language IN ('simple'::regconfig, 'english'::regconfig)),
  search_vector tsvector GENERATED ALWAYS AS (
    setweight(to_tsvector(language, title), 'A') ||
    setweight(to_tsvector(language, body), 'B')
  ) STORED,
  PRIMARY KEY (namespace, tenant_id, owner_id, document_type, document_id)
);
CREATE INDEX "lenso_search_documents_fts" ON "lenso_search_documents" USING gin (search_vector);
CREATE INDEX "lenso_search_documents_scope" ON "lenso_search_documents" (namespace, tenant_id, owner_id, language);
