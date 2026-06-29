drop index if exists media_assets_import_job_file;

alter table media_assets add column queued_file_id text;

update media_assets
set queued_file_id = id
where queued_file_id is null;

create unique index if not exists media_assets_import_job_input
on media_assets(import_job_id, queued_file_id)
where import_job_id is not null and queued_file_id is not null;

delete from timeline_segments
where rowid not in (
  select min(rowid)
  from timeline_segments
  group by project_id, source_id, start_ms, end_ms
);

create unique index if not exists timeline_segment_source_offset
on timeline_segments(project_id, source_id, start_ms, end_ms);
