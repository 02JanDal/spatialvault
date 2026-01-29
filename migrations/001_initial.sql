-- migrations/001_initial.sql

-- Metadata schema for SpatialVault internals
CREATE SCHEMA IF NOT EXISTS spatialvault;

-- Enable PostGIS extension
CREATE EXTENSION IF NOT EXISTS postgis;

-- Service role (create if not exists)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'spatialvault_service') THEN
        CREATE ROLE spatialvault_service WITH LOGIN;
    END IF;
END
$$;

-- Collections registry (minimal - computed fields derived on demand)
CREATE TABLE IF NOT EXISTS spatialvault.collections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    canonical_name TEXT UNIQUE NOT NULL,   -- e.g., "jan:folder:collection"
    owner TEXT NOT NULL,                   -- user or group role name
    schema_name TEXT NOT NULL,             -- PostgreSQL schema (e.g., "jan")
    table_name TEXT NOT NULL,              -- table within schema
    collection_type TEXT NOT NULL CHECK (collection_type IN ('vector', 'raster', 'pointcloud')),
    title TEXT NOT NULL,                   -- human-readable; id derived from title or vice-versa
    description TEXT,
    version BIGINT NOT NULL DEFAULT 1,     -- For ETag generation
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
    -- CRS: derived from geometry column SRID
    -- bbox: computed via ST_Extent on items/features
    -- temporal_extent: computed via MIN/MAX on datetime column
    -- schema_definition: introspected from table structure
);

CREATE INDEX IF NOT EXISTS idx_collections_owner ON spatialvault.collections(owner);
CREATE INDEX IF NOT EXISTS idx_collections_type ON spatialvault.collections(collection_type);

-- Aliases for renamed/moved collections (redirects)
CREATE TABLE IF NOT EXISTS spatialvault.collection_aliases (
    old_name TEXT PRIMARY KEY,
    new_name TEXT NOT NULL REFERENCES spatialvault.collections(canonical_name) ON UPDATE CASCADE,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Single pre-created table for ALL raster/pointcloud items (across all collections)
CREATE TABLE IF NOT EXISTS spatialvault.items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    collection_id UUID NOT NULL REFERENCES spatialvault.collections(id) ON DELETE CASCADE,
    geometry geometry(Geometry, 4326) NOT NULL,  -- footprint/bounds
    datetime TIMESTAMPTZ,
    properties JSONB,
    version BIGINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_items_collection ON spatialvault.items(collection_id);
CREATE INDEX IF NOT EXISTS idx_items_geometry ON spatialvault.items USING GIST(geometry);
CREATE INDEX IF NOT EXISTS idx_items_datetime ON spatialvault.items(datetime);

-- Normalized assets table (one row per asset per item)
CREATE TABLE IF NOT EXISTS spatialvault.assets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    item_id UUID NOT NULL REFERENCES spatialvault.items(id) ON DELETE CASCADE,
    key TEXT NOT NULL,                -- asset key, e.g., "data", "thumbnail", "metadata"
    href TEXT NOT NULL,               -- S3 URI or URL
    type TEXT,                        -- media type, e.g., "image/tiff; application=geotiff; profile=cloud-optimized"
    title TEXT,
    description TEXT,
    roles TEXT[],                     -- e.g., ARRAY['data'], ARRAY['thumbnail']
    file_size BIGINT,
    extra_fields JSONB,               -- additional STAC asset fields
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(item_id, key)
);

CREATE INDEX IF NOT EXISTS idx_assets_item ON spatialvault.assets(item_id);

-- OGC API Processes jobs (async operations like file import)
CREATE TABLE IF NOT EXISTS spatialvault.processes_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    process_id TEXT NOT NULL,              -- e.g., 'import-raster', 'import-pointcloud'
    status TEXT NOT NULL DEFAULT 'accepted'
        CHECK (status IN ('accepted', 'running', 'successful', 'failed', 'dismissed')),
    owner TEXT NOT NULL,
    type TEXT DEFAULT 'process',
    message TEXT,
    progress INTEGER CHECK (progress >= 0 AND progress <= 100),
    inputs JSONB,
    outputs JSONB,
    created TIMESTAMPTZ DEFAULT NOW(),
    started TIMESTAMPTZ,
    finished TIMESTAMPTZ,
    updated TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_jobs_owner ON spatialvault.processes_jobs(owner);
CREATE INDEX IF NOT EXISTS idx_jobs_status ON spatialvault.processes_jobs(status);

-- Function to auto-create user/group roles
CREATE OR REPLACE FUNCTION spatialvault.ensure_role(role_name TEXT, is_group BOOLEAN DEFAULT FALSE)
RETURNS VOID AS $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = role_name) THEN
        EXECUTE format('CREATE ROLE %I WITH NOLOGIN', role_name);
        EXECUTE format('CREATE SCHEMA IF NOT EXISTS %I AUTHORIZATION %I', role_name, role_name);
    END IF;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;

-- Grant necessary permissions to service role
GRANT USAGE ON SCHEMA spatialvault TO spatialvault_service;
GRANT ALL ON ALL TABLES IN SCHEMA spatialvault TO spatialvault_service;
GRANT ALL ON ALL SEQUENCES IN SCHEMA spatialvault TO spatialvault_service;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA spatialvault TO spatialvault_service;

-- Vector collections use per-user schema tables (created dynamically):
-- Example for user "jan": CREATE TABLE jan.cities (
--     id UUID PRIMARY KEY,
--     geometry geometry(Point, 4326) NOT NULL,
--     properties JSONB,
--     version BIGINT DEFAULT 1,
--     created_at TIMESTAMPTZ DEFAULT NOW(),
--     updated_at TIMESTAMPTZ DEFAULT NOW()
-- );
