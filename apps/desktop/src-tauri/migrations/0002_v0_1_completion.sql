create table if not exists project_keys (
  project_id text primary key references projects(id) on delete cascade,
  wrapped_key_json text not null,
  created_at text not null
);

create table if not exists media_keys (
  asset_id text primary key references media_assets(id) on delete cascade,
  project_id text not null references projects(id) on delete cascade,
  wrapped_key_json text not null,
  created_at text not null
);

create table if not exists media_chunks (
  id text primary key,
  asset_id text not null references media_assets(id) on delete cascade,
  chunk_index integer not null,
  start_ms integer not null,
  end_ms integer not null,
  overlap_ms integer not null default 0,
  sha256 text not null,
  state text not null,
  retry_count integer not null default 0,
  provider_status text not null default 'pending',
  encrypted_transcript_path text,
  error_code text,
  unique(asset_id, chunk_index)
);

create table if not exists provider_configurations (
  provider_id text primary key,
  encrypted_path text not null,
  configured_fields_json text not null default '[]',
  updated_at text not null
);

create table if not exists app_settings (
  key text primary key,
  value_json text not null,
  updated_at text not null
);

alter table jobs add column asset_id text;
alter table jobs add column stage text not null default 'queued';
alter table jobs add column progress real not null default 0;
alter table jobs add column error_code text;
alter table jobs add column started_at text;
alter table jobs add column completed_at text;
alter table media_assets add column import_job_id text;
alter table generation_runs add column prompt_version text not null default 'unknown';
alter table generation_runs add column schema_version text not null default 'unknown';
alter table generation_runs add column source_ids_json text not null default '[]';
alter table generation_runs add column started_at text;
alter table generation_runs add column completed_at text;
create unique index if not exists media_assets_import_job_file on media_assets(import_job_id, original_file_name) where import_job_id is not null;
