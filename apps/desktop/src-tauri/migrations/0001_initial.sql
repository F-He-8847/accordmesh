create table if not exists projects (
  id text primary key,
  title text not null,
  origin text not null,
  status text not null,
  created_at text not null,
  updated_at text not null,
  realtime_session_id text
);

create table if not exists realtime_sessions (
  id text primary key,
  project_id text not null references projects(id) on delete cascade,
  mode text not null,
  started_at text not null,
  ended_at text,
  status text not null
);

create table if not exists timeline_segments (
  id text primary key,
  project_id text not null references projects(id) on delete cascade,
  source_id text not null,
  track_role text not null,
  start_ms integer not null,
  end_ms integer not null,
  encrypted_payload_path text not null,
  transcript_status text not null,
  created_at text not null
);

create table if not exists media_assets (
  id text primary key,
  project_id text not null references projects(id) on delete cascade,
  kind text not null,
  original_file_name text not null,
  imported_at text not null,
  duration_ms integer,
  sha256 text not null,
  encrypted_path text not null,
  processing_status text not null
);

create table if not exists artifacts (
  id text primary key,
  project_id text not null references projects(id) on delete cascade,
  artifact_type text not null,
  source_ids_json text not null,
  schema_version text not null,
  prompt_version text not null,
  provider_id text not null,
  model_id text not null,
  app_version text not null,
  created_at text not null,
  status text not null,
  encrypted_payload_path text not null
);

create table if not exists generation_runs (
  id text primary key,
  project_id text not null references projects(id) on delete cascade,
  artifact_id text,
  provider_id text not null,
  model_id text not null,
  status text not null,
  error_code text,
  created_at text not null
);

create table if not exists jobs (
  id text primary key,
  project_id text references projects(id) on delete cascade,
  kind text not null,
  status text not null,
  priority integer not null default 50,
  payload_json text not null default '{}',
  retry_count integer not null default 0,
  created_at text not null,
  updated_at text not null
);

create table if not exists provider_credentials (
  provider_id text primary key,
  encrypted_path text not null,
  updated_at text not null
);
