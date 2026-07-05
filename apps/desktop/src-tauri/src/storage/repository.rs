use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto;
use crate::media;
use crate::projects::types::*;

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct Repository {
    data_dir: PathBuf,
    db_path: PathBuf,
}

pub struct StoredMediaChunk {
    pub id: String,
    pub chunk_index: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    pub overlap_ms: i64,
    pub sha256: String,
    pub state: String,
    pub retry_count: i64,
    pub provider_status: String,
    pub encrypted_transcript_path: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum EnsureMediaChunkError {
    #[error("ERR_MEDIA_CHANGED")]
    Contract,
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum DeleteProjectError {
    #[error("ERR_ACTIVE_JOB")]
    ActiveJob,
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
}

impl Repository {
    pub fn new(data_dir: PathBuf) -> Result<Self, std::io::Error> {
        std::fs::create_dir_all(&data_dir)?;
        Ok(Self {
            db_path: data_dir.join("app.sqlite"),
            data_dir,
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
    pub fn realtime_pending_dir(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id)
            .join("runtime")
            .join("realtime_pending")
    }
    pub fn has_user_data(&self) -> rusqlite::Result<bool> {
        self.conn()?.query_row("select (select count(*) from projects)+(select count(*) from provider_credentials)+(select count(*) from provider_configurations)>0",[],|row|row.get(0))
    }

    pub fn initialize(&self) -> rusqlite::Result<()> {
        for directory in ["projects", "credentials", "temp"] {
            std::fs::create_dir_all(self.data_dir.join(directory)).ok();
        }
        let mut conn = self.conn()?;
        let journal_mode: String =
            conn.query_row("pragma journal_mode=WAL", [], |row| row.get(0))?;
        if !journal_mode.eq_ignore_ascii_case("wal") {
            return Err(rusqlite::Error::InvalidQuery);
        }
        conn.execute_batch(include_str!("../../migrations/0001_initial.sql"))?;
        conn.execute_batch("create table if not exists schema_migrations(version integer primary key, applied_at text not null);")?;
        let version_two_applied: bool = conn.query_row(
            "select exists(select 1 from schema_migrations where version=2)",
            [],
            |row| row.get(0),
        )?;
        if !version_two_applied {
            apply_migration_0002(&mut conn)?;
        }
        let version_three_applied: bool = conn.query_row(
            "select exists(select 1 from schema_migrations where version=3)",
            [],
            |row| row.get(0),
        )?;
        if !version_three_applied {
            let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute_batch(include_str!(
                "../../migrations/0003_integration_recovery.sql"
            ))?;
            transaction.execute(
                "insert into schema_migrations(version, applied_at) values(3, ?1)",
                params![now()],
            )?;
            transaction.commit()?;
        }
        conn.execute(
            "update jobs set status='resumable', stage=case when stage='queued' then 'interrupted' else stage end, updated_at=?1 where status='running'",
            params![now()],
        )?;
        conn.execute(
            "update realtime_sessions set status='interrupted', ended_at=?1 where status in ('starting','running','paused')",
            params![now()],
        )?;
        conn.execute(
            "update projects set status='failed', updated_at=?1 where status='active' and realtime_session_id in (select id from realtime_sessions where status='interrupted')",
            params![now()],
        )?;
        self.cleanup_stale_temporary_files(&conn);
        Ok(())
    }

    fn cleanup_stale_temporary_files(&self, connection: &Connection) {
        let mut retained = std::collections::HashSet::new();
        if let Ok(mut statement) = connection.prepare(
            "select payload_json from jobs where status in ('queued','running','resumable')",
        ) {
            if let Ok(rows) = statement.query_map([], |row| row.get::<_, String>(0)) {
                for payload in rows.flatten() {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) {
                        if let Some(files) = value.get("files").and_then(|value| value.as_array()) {
                            for file in files {
                                if let Some(path) =
                                    file.get("tempPath").and_then(|value| value.as_str())
                                {
                                    retained.insert(PathBuf::from(path));
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir(self.data_dir.join("temp")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !retained.contains(&path) {
                    if path.is_dir() {
                        let _ = std::fs::remove_dir_all(path);
                    } else {
                        let _ = std::fs::remove_file(path);
                    }
                }
            }
        }
    }

    pub fn create_project(
        &self,
        title: &str,
        origin: ProjectOrigin,
        status: ProjectStatus,
        master_key: &[u8],
    ) -> Result<MeetingProject, Box<dyn std::error::Error>> {
        let id = Uuid::new_v4().to_string();
        let timestamp = now();
        let project_key = crypto::random_key();
        let wrapped = crypto::seal(master_key, &project_key)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "insert into projects(id, title, origin, status, created_at, updated_at) values(?1, ?2, ?3, ?4, ?5, ?5)",
            params![id, title, encode(&origin), encode(&status), timestamp],
        )?;
        tx.execute(
            "insert into project_keys(project_id, wrapped_key_json, created_at) values(?1, ?2, ?3)",
            params![id, serde_json::to_string(&wrapped)?, timestamp],
        )?;
        tx.commit()?;
        std::fs::create_dir_all(self.project_dir(&id))?;
        Ok(self.project(&id)?)
    }

    pub fn project_key(
        &self,
        project_id: &str,
        master_key: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, Box<dyn std::error::Error>> {
        let wrapped: String = self.conn()?.query_row(
            "select wrapped_key_json from project_keys where project_id=?1",
            params![project_id],
            |row| row.get(0),
        )?;
        let sealed = crypto::from_slice(wrapped.as_bytes())?;
        Ok(Zeroizing::new(crypto::open(master_key, &sealed)?))
    }

    pub fn ensure_legacy_keys(&self, master_key: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "select id from projects where id not in (select project_id from project_keys)",
        )?;
        let projects = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        drop(conn);
        for project_id in projects {
            self.conn()?.execute(
                "insert into project_keys(project_id,wrapped_key_json,created_at) values(?1,?2,?3)",
                params![
                    project_id,
                    serde_json::to_string(&crypto::seal(master_key, master_key)?)?,
                    now()
                ],
            )?;
        }
        let conn = self.conn()?;
        let mut stmt=conn.prepare("select id,project_id from media_assets where id not in (select asset_id from media_keys)")?;
        let media = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        drop(conn);
        for (asset_id, project_id) in media {
            let key = self.project_key(&project_id, master_key)?;
            self.conn()?.execute("insert into media_keys(asset_id,project_id,wrapped_key_json,created_at) values(?1,?2,?3,?4)",params![asset_id,project_id,serde_json::to_string(&crypto::seal(&key,&key)?)?,now()])?;
        }
        Ok(())
    }

    pub fn create_realtime_session(
        &self,
        project_id: &str,
        mode: RealtimeMode,
    ) -> rusqlite::Result<RealtimeSession> {
        let id = Uuid::new_v4().to_string();
        let timestamp = now();
        self.conn()?.execute(
            "insert into realtime_sessions(id, project_id, mode, started_at, status) values(?1, ?2, ?3, ?4, 'starting')",
            params![id, project_id, encode(&mode), timestamp],
        )?;
        self.conn()?.execute(
            "update projects set realtime_session_id=?1 where id=?2",
            params![id, project_id],
        )?;
        self.set_realtime_status(&id, RealtimeSessionStatus::Running)?;
        Ok(self
            .realtime_session(project_id)?
            .expect("inserted session"))
    }

    pub fn fail_realtime_session(
        &self,
        session_id: &str,
        project_id: &str,
    ) -> rusqlite::Result<()> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let timestamp = now();
        tx.execute(
            "update realtime_sessions set status='interrupted', ended_at=?1 where id=?2",
            params![&timestamp, session_id],
        )?;
        tx.execute(
            "update projects set status='failed', updated_at=?1 where id=?2",
            params![&timestamp, project_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn set_realtime_status(
        &self,
        session_id: &str,
        status: RealtimeSessionStatus,
    ) -> rusqlite::Result<()> {
        let completed = matches!(
            status,
            RealtimeSessionStatus::Completed | RealtimeSessionStatus::Interrupted
        );
        self.conn()?.execute(
            "update realtime_sessions set status=?1, ended_at=case when ?2 then ?3 else ended_at end where id=?4",
            params![encode(&status), completed, now(), session_id],
        )?;
        Ok(())
    }

    pub fn insert_segment(
        &self,
        segment: &TimelineSegment,
        project_key: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = self.project_dir(&segment.project_id).join("transcripts");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json.enc", segment.id));
        write_sealed(
            &path,
            &crypto::seal(project_key, &serde_json::to_vec(segment)?)?,
        )?;
        self.conn()?.execute(
            "insert or replace into timeline_segments(id, project_id, source_id, track_role, start_ms, end_ms, encrypted_payload_path, transcript_status, created_at)
             values(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![segment.id, segment.project_id, segment.source_id, encode(&segment.track_role), segment.start_ms, segment.end_ms, path.to_string_lossy(), segment.transcript_status, segment.created_at],
        )?;
        self.touch_project(&segment.project_id)?;
        Ok(())
    }

    pub async fn import_media_asset(
        &self,
        project_id: &str,
        import_job_id: &str,
        queued_file_id: &str,
        name: &str,
        kind: MediaKind,
        mime_type: Option<String>,
        source: &Path,
        project_key: &[u8],
    ) -> Result<MediaAsset, &'static str> {
        let id = Uuid::new_v4().to_string();
        let dir = self.project_dir(project_id).join("media");
        std::fs::create_dir_all(&dir).map_err(|_| "ERR_IO")?;
        let path = dir.join(format!("{id}.ammedia"));
        let media_key = crypto::random_key();
        let manifest =
            media::encrypt_managed_file(source, &path, &media_key, name, mime_type, kind).await?;
        let wrapped = crypto::seal(project_key, &media_key).map_err(|_| "ERR_CRYPTO")?;
        let timestamp = now();
        let mut connection = self.conn().map_err(|_| "ERR_STORAGE")?;
        let transaction = connection.transaction().map_err(|_| "ERR_STORAGE")?;
        transaction.execute(
            "insert into media_assets(id,project_id,kind,original_file_name,imported_at,sha256,encrypted_path,processing_status,import_job_id,queued_file_id) values(?1,?2,?3,?4,?5,?6,?7,'processing',?8,?9)",
            params![id,project_id,encode(&kind),name,timestamp,manifest.sha256,path.to_string_lossy(),import_job_id,queued_file_id],
        ).map_err(|_| "ERR_STORAGE")?;
        transaction.execute(
            "insert into media_keys(asset_id,project_id,wrapped_key_json,created_at) values(?1,?2,?3,?4)",
            params![id,project_id,serde_json::to_string(&wrapped).map_err(|_| "ERR_JSON")?,timestamp],
        ).map_err(|_| "ERR_STORAGE")?;
        transaction.commit().map_err(|_| "ERR_STORAGE")?;
        self.touch_project(project_id).map_err(|_| "ERR_STORAGE")?;
        Ok(MediaAsset {
            id,
            project_id: project_id.into(),
            kind,
            original_file_name: name.into(),
            imported_at: timestamp,
            duration_ms: None,
            sha256: manifest.sha256,
            processing_status: "processing".into(),
        })
    }

    pub async fn materialize_media_asset(
        &self,
        asset_id: &str,
        project_key: &[u8],
    ) -> Result<media::TemporaryPath, &'static str> {
        let (encrypted_path, wrapped, name): (String, String, String) = self.conn().map_err(|_| "ERR_STORAGE")?.query_row(
            "select m.encrypted_path,k.wrapped_key_json,m.original_file_name from media_assets m join media_keys k on k.asset_id=m.id where m.id=?1",
            params![asset_id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        ).map_err(|_| "ERR_STORAGE")?;
        let media_key = Zeroizing::new(
            crypto::open(
                project_key,
                &crypto::from_slice(wrapped.as_bytes())
                    .map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?,
            )
            .map_err(|_| "ERR_ENCRYPTED_DATA_CORRUPT")?,
        );
        let extension = Path::new(&name)
            .extension()
            .and_then(|value| value.to_str())
            .filter(|value| value.chars().all(|c| c.is_ascii_alphanumeric()))
            .unwrap_or("bin");
        let path = self.data_dir.join("temp").join(format!(
            "materialized-{}.{}",
            Uuid::new_v4(),
            extension
        ));
        let temporary = media::TemporaryPath::from_existing(path);
        media::decrypt_managed_file(Path::new(&encrypted_path), temporary.path(), &media_key)
            .await?;
        Ok(temporary)
    }

    pub fn media_for_job(
        &self,
        job_id: &str,
        queued_file_id: &str,
    ) -> rusqlite::Result<Option<MediaAsset>> {
        self.conn()?.query_row("select id,project_id,kind,original_file_name,imported_at,duration_ms,sha256,processing_status from media_assets where import_job_id=?1 and queued_file_id=?2",params![job_id,queued_file_id],|row|Ok(MediaAsset{id:row.get(0)?,project_id:row.get(1)?,kind:decode(&row.get::<_,String>(2)?),original_file_name:row.get(3)?,imported_at:row.get(4)?,duration_ms:row.get(5)?,sha256:row.get(6)?,processing_status:row.get(7)?})).optional()
    }

    pub fn media_for_job_legacy(
        &self,
        job_id: &str,
        original_file_name: &str,
    ) -> rusqlite::Result<Option<MediaAsset>> {
        self.conn()?.query_row(
            "select id,project_id,kind,original_file_name,imported_at,duration_ms,sha256,processing_status from media_assets where import_job_id=?1 and original_file_name=?2 order by imported_at desc limit 1",
            params![job_id,original_file_name],
            |row|Ok(MediaAsset{id:row.get(0)?,project_id:row.get(1)?,kind:decode(&row.get::<_,String>(2)?),original_file_name:row.get(3)?,imported_at:row.get(4)?,duration_ms:row.get(5)?,sha256:row.get(6)?,processing_status:row.get(7)?}),
        ).optional()
    }

    pub fn update_media_status(
        &self,
        asset_id: &str,
        status: &str,
        duration_ms: Option<i64>,
    ) -> rusqlite::Result<()> {
        self.conn()?.execute("update media_assets set processing_status=?1, duration_ms=coalesce(?2,duration_ms) where id=?3", params![status, duration_ms, asset_id])?;
        Ok(())
    }
    pub fn finalize_incomplete_media_for_job(
        &self,
        job_id: &str,
        status: &str,
    ) -> rusqlite::Result<usize> {
        self.conn()?.execute(
            "update media_assets set processing_status=?1 where import_job_id=?2 and processing_status not in ('ready','attached','failed')",
            params![status, job_id],
        )
    }

    pub fn delete_media_asset(&self, asset_id: &str) -> rusqlite::Result<()> {
        let path: Option<String> = self
            .conn()?
            .query_row(
                "select encrypted_path from media_assets where id=?1",
                params![asset_id],
                |row| row.get(0),
            )
            .optional()?;
        let mut connection = self.conn()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "delete from media_chunks where asset_id=?1",
            params![asset_id],
        )?;
        transaction.execute(
            "delete from media_keys where asset_id=?1",
            params![asset_id],
        )?;
        transaction.execute("delete from media_assets where id=?1", params![asset_id])?;
        transaction.commit()?;
        if let Some(path) = path {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    pub fn segments_for_source(
        &self,
        project_id: &str,
        source_id: &str,
        key: &[u8],
    ) -> Result<Vec<TimelineSegment>, Box<dyn std::error::Error>> {
        Ok(self
            .segments(project_id, key)?
            .into_iter()
            .filter(|segment| segment.source_id == source_id)
            .collect())
    }

    pub fn ensure_media_chunk(
        &self,
        asset_id: &str,
        index: i64,
        start_ms: i64,
        end_ms: i64,
        overlap_ms: i64,
        sha256: &str,
    ) -> Result<StoredMediaChunk, EnsureMediaChunkError> {
        if let Some(existing) = self.media_chunk(asset_id, index)? {
            if existing.start_ms != start_ms
                || existing.end_ms != end_ms
                || existing.overlap_ms != overlap_ms
                || existing.sha256 != sha256
            {
                return Err(EnsureMediaChunkError::Contract);
            }
            return Ok(existing);
        }
        let id = Uuid::new_v4().to_string();
        self.conn()?.execute(
            "insert into media_chunks(id,asset_id,chunk_index,start_ms,end_ms,overlap_ms,sha256,state,provider_status) values(?1,?2,?3,?4,?5,?6,?7,'ready','pending')",
            params![id,asset_id,index,start_ms,end_ms,overlap_ms,sha256],
        )?;
        self.media_chunk(asset_id, index)?
            .ok_or_else(|| EnsureMediaChunkError::Sql(rusqlite::Error::QueryReturnedNoRows))
    }

    pub fn media_chunks(&self, asset_id: &str) -> rusqlite::Result<Vec<StoredMediaChunk>> {
        let connection = self.conn()?;
        let mut statement=connection.prepare("select id,chunk_index,start_ms,end_ms,overlap_ms,sha256,state,retry_count,provider_status,encrypted_transcript_path,error_code from media_chunks where asset_id=?1 order by chunk_index")?;
        let rows = statement.query_map(params![asset_id], |row| {
            Ok(StoredMediaChunk {
                id: row.get(0)?,
                chunk_index: row.get(1)?,
                start_ms: row.get(2)?,
                end_ms: row.get(3)?,
                overlap_ms: row.get(4)?,
                sha256: row.get(5)?,
                state: row.get(6)?,
                retry_count: row.get(7)?,
                provider_status: row.get(8)?,
                encrypted_transcript_path: row.get(9)?,
                error_code: row.get(10)?,
            })
        })?;
        rows.collect()
    }

    fn media_chunk(
        &self,
        asset_id: &str,
        index: i64,
    ) -> rusqlite::Result<Option<StoredMediaChunk>> {
        self.conn()?.query_row(
            "select id,chunk_index,start_ms,end_ms,overlap_ms,sha256,state,retry_count,provider_status,encrypted_transcript_path,error_code from media_chunks where asset_id=?1 and chunk_index=?2",
            params![asset_id,index],
            |row|Ok(StoredMediaChunk{id:row.get(0)?,chunk_index:row.get(1)?,start_ms:row.get(2)?,end_ms:row.get(3)?,overlap_ms:row.get(4)?,sha256:row.get(5)?,state:row.get(6)?,retry_count:row.get(7)?,provider_status:row.get(8)?,encrypted_transcript_path:row.get(9)?,error_code:row.get(10)?}),
        ).optional()
    }

    pub fn mark_chunk_running(&self, chunk_id: &str) -> rusqlite::Result<()> {
        self.conn()?.execute("update media_chunks set state='running',provider_status='running',retry_count=retry_count+1,error_code=null where id=?1 and state!='completed'",params![chunk_id])?;
        Ok(())
    }

    pub fn load_chunk_transcript(
        &self,
        chunk: &StoredMediaChunk,
        project_key: &[u8],
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let path = chunk
            .encrypted_transcript_path
            .as_ref()
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        let envelope = crypto::from_slice(&std::fs::read(path)?)?;
        Ok(serde_json::from_slice(&crypto::open(
            project_key,
            &envelope,
        )?)?)
    }

    pub fn update_chunk_result(
        &self,
        chunk_id: &str,
        status: &str,
        error_code: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.conn()?.execute("update media_chunks set provider_status=?1, state=case when ?2 is null then 'completed' else 'failed' end, error_code=?2 where id=?3", params![status, error_code, chunk_id])?;
        Ok(())
    }

    pub fn store_chunk_transcript(
        &self,
        chunk_id: &str,
        project_id: &str,
        transcript: &serde_json::Value,
        project_key: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = self.project_dir(project_id).join("transcripts/chunks");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{chunk_id}.json.enc"));
        write_sealed(
            &path,
            &crypto::seal(project_key, &serde_json::to_vec(transcript)?)?,
        )?;
        self.conn()?.execute("update media_chunks set provider_status='completed',state='completed',encrypted_transcript_path=?1,error_code=null where id=?2",params![path.to_string_lossy(),chunk_id])?;
        Ok(())
    }

    pub fn begin_generation_run(
        &self,
        project_id: &str,
        provider_id: &str,
        model_id: &str,
        prompt_version: &str,
        schema_version: &str,
        source_ids: &[String],
    ) -> rusqlite::Result<String> {
        let id = Uuid::new_v4().to_string();
        let timestamp = now();
        self.conn()?.execute(
            "insert into generation_runs(id, project_id, provider_id, model_id, prompt_version, schema_version, source_ids_json, status, created_at, started_at)
             values(?1, ?2, ?3, ?4, ?5, ?6, ?7, 'running', ?8, ?8)",
            params![id, project_id, provider_id, model_id, prompt_version, schema_version, serde_json::to_string(source_ids).unwrap_or_else(|_| "[]".into()), timestamp],
        )?;
        Ok(id)
    }

    pub fn generation_run_status(&self, run_id: &str) -> rusqlite::Result<Option<String>> {
        self.conn()?
            .query_row(
                "select status from generation_runs where id=?1",
                params![run_id],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn begin_or_restart_generation_run(
        &self,
        run_id: &str,
        project_id: &str,
        provider_id: &str,
        model_id: &str,
        prompt_version: &str,
        schema_version: &str,
        source_ids: &[String],
    ) -> rusqlite::Result<()> {
        let timestamp = now();
        let source_ids_json = serde_json::to_string(source_ids).unwrap_or_else(|_| "[]".into());
        match self.generation_run_status(run_id)? {
            None => {
                self.conn()?.execute(
                    "insert into generation_runs(id, project_id, provider_id, model_id, prompt_version, schema_version, source_ids_json, status, created_at, started_at)
                     values(?1, ?2, ?3, ?4, ?5, ?6, ?7, 'running', ?8, ?8)",
                    params![run_id, project_id, provider_id, model_id, prompt_version, schema_version, source_ids_json, timestamp],
                )?;
            }
            Some(status) if status != "completed" => {
                self.conn()?.execute(
                    "update generation_runs
                     set artifact_id=null, provider_id=?1, model_id=?2, prompt_version=?3, schema_version=?4, source_ids_json=?5, status='running', error_code=null, started_at=?6, completed_at=null
                     where id=?7",
                    params![provider_id, model_id, prompt_version, schema_version, source_ids_json, timestamp, run_id],
                )?;
            }
            Some(_) => return Err(rusqlite::Error::InvalidQuery),
        }
        Ok(())
    }

    pub fn completed_artifact_by_id(
        &self,
        project_id: &str,
        artifact_id: &str,
        project_key: &[u8],
    ) -> Result<Option<AnalysisArtifact>, Box<dyn std::error::Error>> {
        Ok(self
            .artifacts(project_id, project_key)?
            .into_iter()
            .find(|artifact| artifact.id == artifact_id && artifact.status == "completed"))
    }

    pub fn complete_generation(
        &self,
        run_id: &str,
        artifact: &AnalysisArtifact,
        project_key: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = self.project_dir(&artifact.project_id).join("artifacts");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json.enc", artifact.id));
        write_sealed(
            &path,
            &crypto::seal(project_key, &serde_json::to_vec(&artifact.payload)?)?,
        )?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "insert into artifacts(id, project_id, artifact_type, source_ids_json, schema_version, prompt_version, provider_id, model_id, app_version, created_at, status, encrypted_payload_path)
             values(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'completed', ?11)",
            params![artifact.id, artifact.project_id, artifact.artifact_type, serde_json::to_string(&artifact.source_ids)?, artifact.schema_version, artifact.prompt_version, artifact.provider_id, artifact.model_id, artifact.app_version, artifact.created_at, path.to_string_lossy()],
        )?;
        tx.execute(
            "update generation_runs
             set artifact_id=?1,
                 provider_id=?2,
                 model_id=?3,
                 prompt_version=?4,
                 schema_version=?5,
                 source_ids_json=?6,
                 status='completed',
                 completed_at=?7
             where id=?8",
            params![
                artifact.id,
                artifact.provider_id,
                artifact.model_id,
                artifact.prompt_version,
                artifact.schema_version,
                serde_json::to_string(&artifact.source_ids)?,
                now(),
                run_id
            ],
        )?;
        tx.execute(
            "update projects set updated_at=?1 where id=?2",
            params![now(), artifact.project_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn fail_generation(&self, run_id: &str, error_code: &str) -> rusqlite::Result<()> {
        self.conn()?.execute("update generation_runs set status='failed', error_code=?1, completed_at=?2 where id=?3", params![error_code, now(), run_id])?;
        Ok(())
    }

    pub fn queue_job(
        &self,
        project_id: &str,
        asset_id: Option<&str>,
        kind: &str,
        priority: i64,
        payload: &serde_json::Value,
    ) -> rusqlite::Result<String> {
        let id = Uuid::new_v4().to_string();
        let timestamp = now();
        self.conn()?.execute(
            "insert into jobs(id, project_id, asset_id, kind, status, stage, progress, priority, payload_json, retry_count, created_at, updated_at)
             values(?1, ?2, ?3, ?4, 'queued', 'queued', 0, ?5, ?6, 0, ?7, ?7)",
            params![id, project_id, asset_id, kind, priority, payload.to_string(), timestamp],
        )?;
        Ok(id)
    }

    pub fn update_job(
        &self,
        job_id: &str,
        status: &str,
        stage: &str,
        progress: f64,
        error_code: Option<&str>,
    ) -> rusqlite::Result<()> {
        let timestamp = now();
        self.conn()?.execute(
            "update jobs set status=?1, stage=?2, progress=?3, error_code=?4,
             started_at=case when ?1='running' then coalesce(started_at,?5) else started_at end,
             completed_at=case when ?1 in ('completed','failed','cancelled') then ?5 else completed_at end, updated_at=?5 where id=?6",
            params![status, stage, progress, error_code, timestamp, job_id],
        )?;
        Ok(())
    }

    pub fn cancel_job(&self, job_id: &str) -> rusqlite::Result<()> {
        self.update_job(
            job_id,
            "cancelled",
            "cancelled",
            0.0,
            Some("ERR_JOB_CANCELLED"),
        )
    }

    pub fn request_job_cancel(&self, job_id: &str) -> rusqlite::Result<()> {
        self.update_job(job_id, "cancelling", "cancelling", 0.0, None)
    }

    pub fn job_cancelled(&self, job_id: &str) -> rusqlite::Result<bool> {
        self.conn()?.query_row(
            "select status in ('cancelling','cancelled') from jobs where id=?1",
            params![job_id],
            |row| row.get(0),
        )
    }

    pub fn retry_job(&self, job_id: &str) -> rusqlite::Result<bool> {
        let changed=self.conn()?.execute(
            "update jobs set status='queued', stage='queued', error_code=null, completed_at=null, retry_count=retry_count+1, updated_at=?1 where id=?2 and status in ('failed','resumable','cancelled')",
            params![now(),job_id],
        )?;
        Ok(changed == 1)
    }

    pub fn job_status(&self, job_id: &str) -> rusqlite::Result<Option<String>> {
        self.conn()?
            .query_row(
                "select status from jobs where id=?1",
                params![job_id],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn job_payload(&self, job_id: &str) -> rusqlite::Result<(String, serde_json::Value)> {
        self.conn()?.query_row(
            "select project_id, payload_json from jobs where id=?1",
            params![job_id],
            |row| {
                let payload: String = row.get(1)?;
                Ok((
                    row.get(0)?,
                    serde_json::from_str(&payload).unwrap_or_default(),
                ))
            },
        )
    }

    pub fn regeneration_job_for_request(
        &self,
        project_id: &str,
        request_id: &str,
    ) -> rusqlite::Result<Option<String>> {
        self.conn()?.query_row(
            "select id from jobs where project_id=?1 and kind='regenerate' and json_extract(payload_json,'$.requestId')=?2 order by created_at desc limit 1",
            params![project_id, request_id],
            |row| row.get(0),
        ).optional()
    }

    pub fn update_job_payload(
        &self,
        job_id: &str,
        payload: &serde_json::Value,
    ) -> rusqlite::Result<()> {
        self.conn()?.execute(
            "update jobs set payload_json=?1,updated_at=?2 where id=?3",
            params![payload.to_string(), now(), job_id],
        )?;
        Ok(())
    }

    pub fn resumable_job_ids(&self) -> rusqlite::Result<Vec<String>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("select id from jobs where status in ('queued','resumable') order by priority asc, created_at asc")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect()
    }

    pub fn upsert_provider_configuration(
        &self,
        provider_id: &str,
        sealed: &crypto::SealedBytes,
        fields: &[String],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path = self
            .data_dir
            .join("credentials")
            .join(format!("{}.enc", safe_identifier(provider_id)?));
        write_sealed(&path, sealed)?;
        self.conn()?.execute(
            "insert into provider_configurations(provider_id, encrypted_path, configured_fields_json, updated_at) values(?1,?2,?3,?4)
             on conflict(provider_id) do update set encrypted_path=excluded.encrypted_path, configured_fields_json=excluded.configured_fields_json, updated_at=excluded.updated_at",
            params![provider_id, path.to_string_lossy(), serde_json::to_string(fields)?, now()],
        )?;
        Ok(())
    }

    pub fn provider_configuration(
        &self,
        provider_id: &str,
        master_key: &[u8],
    ) -> Result<Option<serde_json::Value>, Box<dyn std::error::Error>> {
        let path: Option<String> = self
            .conn()?
            .query_row(
                "select encrypted_path from provider_configurations where provider_id=?1",
                params![provider_id],
                |row| row.get(0),
            )
            .optional()?;
        match path {
            Some(path) => Ok(Some(serde_json::from_slice(&crypto::open(
                master_key,
                &crypto::from_slice(&std::fs::read(path)?)?,
            )?)?)),
            None => Ok(None),
        }
    }

    pub fn provider_statuses(&self) -> rusqlite::Result<Vec<(String, Vec<String>, String)>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("select provider_id, configured_fields_json, updated_at from provider_configurations order by provider_id")?;
        let rows = stmt.query_map([], |row| {
            let fields: String = row.get(1)?;
            Ok((
                row.get(0)?,
                serde_json::from_str(&fields).unwrap_or_default(),
                row.get(2)?,
            ))
        })?;
        rows.collect()
    }

    pub fn remove_provider_configuration(&self, provider_id: &str) -> rusqlite::Result<()> {
        let path: Option<String> = self
            .conn()?
            .query_row(
                "select encrypted_path from provider_configurations where provider_id=?1",
                params![provider_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(path) = path {
            std::fs::remove_file(path).ok();
        }
        self.conn()?.execute(
            "delete from provider_configurations where provider_id=?1",
            params![provider_id],
        )?;
        Ok(())
    }

    pub fn save_setting(&self, key: &str, value: &serde_json::Value) -> rusqlite::Result<()> {
        self.conn()?.execute("insert into app_settings(key,value_json,updated_at) values(?1,?2,?3) on conflict(key) do update set value_json=excluded.value_json,updated_at=excluded.updated_at", params![key, value.to_string(), now()])?;
        Ok(())
    }

    pub fn settings(&self) -> rusqlite::Result<serde_json::Value> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("select key,value_json from app_settings")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut map = serde_json::Map::new();
        for row in rows {
            let (key, value) = row?;
            map.insert(
                key,
                serde_json::from_str(&value).unwrap_or(serde_json::Value::Null),
            );
        }
        Ok(serde_json::Value::Object(map))
    }

    pub fn list_projects(&self) -> rusqlite::Result<Vec<MeetingProject>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("select id from projects order by created_at desc")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|id| self.project(&id?)).collect()
    }

    pub fn project_detail(
        &self,
        project_id: &str,
        master_key: &[u8],
    ) -> Result<ProjectDetail, Box<dyn std::error::Error>> {
        let key = self.project_key(project_id, master_key)?;
        Ok(ProjectDetail {
            project: self.project(project_id)?,
            timeline: self.segments(project_id, &key)?,
            media_assets: self.media_assets(project_id)?,
            artifacts: self.artifacts(project_id, &key)?,
            generation_runs: self.generation_runs(project_id)?,
            realtime_session: self.realtime_session(project_id)?,
            jobs: self.jobs(project_id)?,
        })
    }

    pub fn rename_project(
        &self,
        project_id: &str,
        title: &str,
    ) -> rusqlite::Result<MeetingProject> {
        self.conn()?.execute(
            "update projects set title=?1, updated_at=?2 where id=?3",
            params![title, now(), project_id],
        )?;
        self.project(project_id)
    }

    pub fn set_project_status(
        &self,
        project_id: &str,
        status: ProjectStatus,
    ) -> rusqlite::Result<()> {
        self.conn()?.execute(
            "update projects set status=?1, updated_at=?2 where id=?3",
            params![encode(&status), now(), project_id],
        )?;
        Ok(())
    }

    pub fn has_realtime_finalization_job(&self, project_id: &str) -> rusqlite::Result<bool> {
        self.conn()?.query_row(
            "select exists(select 1 from jobs where project_id=?1 and kind='realtime_finalize')",
            params![project_id],
            |row| row.get(0),
        )
    }

    pub fn has_active_jobs(&self, project_id: &str) -> rusqlite::Result<bool> {
        self.conn()?.query_row(
            "select exists(select 1 from jobs where project_id=?1 and status in ('queued','running','resumable','cancelling'))",
            params![project_id],
            |row| row.get(0),
        )
    }

    pub fn delete_project(&self, project_id: &str) -> Result<(), DeleteProjectError> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active_job_exists: bool = tx.query_row(
            "select exists(select 1 from jobs where project_id=?1 and status in ('queued','running','resumable','cancelling'))",
            params![project_id],
            |row| row.get(0),
        )?;
        if active_job_exists {
            return Err(DeleteProjectError::ActiveJob);
        }
        for table in ["media_chunks", "media_keys"] {
            tx.execute(
                &format!("delete from {table} where asset_id in (select id from media_assets where project_id=?1)"),
                params![project_id],
            )?;
        }
        for table in [
            "jobs",
            "generation_runs",
            "artifacts",
            "timeline_segments",
            "media_assets",
            "realtime_sessions",
            "project_keys",
        ] {
            tx.execute(
                &format!("delete from {table} where project_id=?1"),
                params![project_id],
            )?;
        }
        tx.execute("delete from projects where id=?1", params![project_id])?;
        tx.commit()?;
        std::fs::remove_dir_all(self.project_dir(project_id)).ok();
        Ok(())
    }

    fn conn(&self) -> rusqlite::Result<Connection> {
        let connection = Connection::open(&self.db_path)?;
        connection.busy_timeout(SQLITE_BUSY_TIMEOUT)?;
        connection.execute_batch("pragma foreign_keys = on;")?;
        Ok(connection)
    }

    fn project(&self, id: &str) -> rusqlite::Result<MeetingProject> {
        self.conn()?.query_row(
            "select id,title,origin,status,created_at,updated_at,realtime_session_id from projects where id=?1", params![id], |row| {
                let project_id: String = row.get(0)?;
                Ok(MeetingProject {
                    media_asset_ids: self.ids("media_assets", &project_id).unwrap_or_default(),
                    timeline_segment_ids: self.ids("timeline_segments", &project_id).unwrap_or_default(),
                    artifact_ids: self.ids("artifacts", &project_id).unwrap_or_default(),
                    generation_run_ids: self.ids("generation_runs", &project_id).unwrap_or_default(),
                    has_comparison: self.has_artifact(&project_id,"intelligent_comparison_report").unwrap_or(false),
                    has_minutes: self.has_artifact(&project_id,"meeting_minutes").unwrap_or(false),
                    id: project_id, title: row.get(1)?, origin: decode(&row.get::<_,String>(2)?), status: decode(&row.get::<_,String>(3)?),
                    created_at: row.get(4)?, updated_at: row.get(5)?, realtime_session_id: row.get(6)?,
                })
            },
        )
    }

    fn ids(&self, table: &str, project_id: &str) -> rusqlite::Result<Vec<String>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "select id from {table} where project_id=?1 order by rowid"
        ))?;
        let rows = stmt.query_map(params![project_id], |row| row.get(0))?;
        rows.collect()
    }
    fn has_artifact(&self, project_id: &str, artifact_type: &str) -> rusqlite::Result<bool> {
        self.conn()?.query_row("select exists(select 1 from artifacts where project_id=?1 and artifact_type=?2 and status='completed')",params![project_id,artifact_type],|row|row.get(0))
    }

    fn segments(
        &self,
        project_id: &str,
        key: &[u8],
    ) -> Result<Vec<TimelineSegment>, Box<dyn std::error::Error>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("select encrypted_payload_path from timeline_segments where project_id=?1 order by start_ms,id")?;
        let mut out = Vec::new();
        for path in stmt.query_map(params![project_id], |row| row.get::<_, String>(0))? {
            out.push(serde_json::from_slice(&crypto::open(
                key,
                &crypto::from_slice(&std::fs::read(path?)?)?,
            )?)?);
        }
        Ok(out)
    }

    fn artifacts(
        &self,
        project_id: &str,
        key: &[u8],
    ) -> Result<Vec<AnalysisArtifact>, Box<dyn std::error::Error>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("select id,artifact_type,source_ids_json,schema_version,prompt_version,provider_id,model_id,app_version,created_at,status,encrypted_payload_path from artifacts where project_id=?1 order by created_at,id")?;
        let rows = stmt.query_map(params![project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (
                id,
                artifact_type,
                source_ids,
                schema_version,
                prompt_version,
                provider_id,
                model_id,
                app_version,
                created_at,
                status,
                path,
            ) = row?;
            let payload = serde_json::from_slice(&crypto::open(
                key,
                &crypto::from_slice(&std::fs::read(path)?)?,
            )?)?;
            out.push(AnalysisArtifact {
                id,
                project_id: project_id.into(),
                artifact_type,
                source_ids: serde_json::from_str(&source_ids).unwrap_or_default(),
                schema_version,
                prompt_version,
                provider_id,
                model_id,
                app_version,
                created_at,
                status,
                payload,
            });
        }
        Ok(out)
    }

    fn media_assets(&self, project_id: &str) -> rusqlite::Result<Vec<MediaAsset>> {
        let conn = self.conn()?;
        let mut stmt=conn.prepare("select id,kind,original_file_name,imported_at,duration_ms,sha256,processing_status from media_assets where project_id=?1 order by imported_at,id")?;
        let rows = stmt.query_map(params![project_id], |row| {
            Ok(MediaAsset {
                id: row.get(0)?,
                project_id: project_id.into(),
                kind: decode(&row.get::<_, String>(1)?),
                original_file_name: row.get(2)?,
                imported_at: row.get(3)?,
                duration_ms: row.get(4)?,
                sha256: row.get(5)?,
                processing_status: row.get(6)?,
            })
        })?;
        rows.collect()
    }

    fn generation_runs(&self, project_id: &str) -> rusqlite::Result<Vec<GenerationRun>> {
        let conn = self.conn()?;
        let mut stmt=conn.prepare("select id,artifact_id,provider_id,model_id,prompt_version,schema_version,source_ids_json,status,error_code,created_at,started_at,completed_at from generation_runs where project_id=?1 order by created_at,id")?;
        let rows = stmt.query_map(params![project_id], |row| {
            let sources: String = row.get(6)?;
            Ok(GenerationRun {
                id: row.get(0)?,
                project_id: project_id.into(),
                artifact_id: row.get(1)?,
                provider_id: row.get(2)?,
                model_id: row.get(3)?,
                prompt_version: row.get(4)?,
                schema_version: row.get(5)?,
                source_ids: serde_json::from_str(&sources).unwrap_or_default(),
                status: row.get(7)?,
                error_code: row.get(8)?,
                created_at: row.get(9)?,
                started_at: row.get(10)?,
                completed_at: row.get(11)?,
            })
        })?;
        rows.collect()
    }

    fn realtime_session(&self, project_id: &str) -> rusqlite::Result<Option<RealtimeSession>> {
        self.conn()?.query_row("select id,mode,started_at,ended_at,status from realtime_sessions where project_id=?1 order by started_at desc limit 1",params![project_id],|row|Ok(RealtimeSession{id:row.get(0)?,project_id:project_id.into(),mode:decode(&row.get::<_,String>(1)?),started_at:row.get(2)?,ended_at:row.get(3)?,status:decode(&row.get::<_,String>(4)?)})).optional()
    }

    fn jobs(&self, project_id: &str) -> rusqlite::Result<Vec<ProcessingJob>> {
        let conn = self.conn()?;
        let mut stmt=conn.prepare("select id,asset_id,kind,status,stage,progress,priority,retry_count,error_code,created_at,started_at,updated_at,completed_at from jobs where project_id=?1 order by created_at desc")?;
        let rows = stmt.query_map(params![project_id], |row| {
            Ok(ProcessingJob {
                id: row.get(0)?,
                project_id: Some(project_id.into()),
                asset_id: row.get(1)?,
                kind: row.get(2)?,
                status: row.get(3)?,
                stage: row.get(4)?,
                progress: row.get(5)?,
                priority: row.get(6)?,
                retry_count: row.get(7)?,
                error_code: row.get(8)?,
                created_at: row.get(9)?,
                started_at: row.get(10)?,
                updated_at: row.get(11)?,
                completed_at: row.get(12)?,
            })
        })?;
        rows.collect()
    }

    fn project_dir(&self, project_id: &str) -> PathBuf {
        self.data_dir.join("projects").join(project_id)
    }
    fn touch_project(&self, project_id: &str) -> rusqlite::Result<()> {
        self.conn()?.execute(
            "update projects set updated_at=?1 where id=?2",
            params![now(), project_id],
        )?;
        Ok(())
    }
}

fn apply_migration_0002(connection: &mut Connection) -> rusqlite::Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for statement in include_str!("../../migrations/0002_v0_1_completion.sql")
        .split(';')
        .map(str::trim)
        .filter(|statement| !statement.is_empty())
    {
        if let Some((table, column)) = added_column_target(statement) {
            if column_exists(&transaction, table, column)? {
                continue;
            }
        }
        transaction.execute_batch(statement)?;
    }
    transaction.execute(
        "insert into schema_migrations(version, applied_at) values(2, ?1)",
        params![now()],
    )?;
    transaction.commit()
}

fn added_column_target(statement: &str) -> Option<(&str, &str)> {
    let tokens = statement.split_whitespace().collect::<Vec<_>>();
    if tokens.len() >= 6
        && tokens[0].eq_ignore_ascii_case("alter")
        && tokens[1].eq_ignore_ascii_case("table")
        && tokens[3].eq_ignore_ascii_case("add")
        && tokens[4].eq_ignore_ascii_case("column")
    {
        Some((tokens[2], tokens[5]))
    } else {
        None
    }
}

fn column_exists(connection: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    connection.query_row(
        "select exists(select 1 from pragma_table_info(?1) where name=?2)",
        params![table, column],
        |row| row.get(0),
    )
}

fn write_sealed(
    path: &Path,
    sealed: &crypto::SealedBytes,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, crypto::to_vec(sealed)?)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

fn safe_identifier(value: &str) -> Result<&str, Box<dyn std::error::Error>> {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Ok(value)
    } else {
        Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "ERR_INVALID_IDENTIFIER").into())
    }
}
fn now() -> String {
    Utc::now().to_rfc3339()
}
fn encode<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".into())
}
fn decode<T: serde::de::DeserializeOwned + Default>(value: &str) -> T {
    serde_json::from_value(serde_json::Value::String(value.into())).unwrap_or_default()
}
impl Default for ProjectOrigin {
    fn default() -> Self {
        Self::UploadOnly
    }
}
impl Default for ProjectStatus {
    fn default() -> Self {
        Self::Failed
    }
}
impl Default for RealtimeMode {
    fn default() -> Self {
        Self::InPerson
    }
}
impl Default for RealtimeSessionStatus {
    fn default() -> Self {
        Self::Interrupted
    }
}
impl Default for TrackRole {
    fn default() -> Self {
        Self::Unknown
    }
}
impl Default for MediaKind {
    fn default() -> Self {
        Self::Transcript
    }
}
