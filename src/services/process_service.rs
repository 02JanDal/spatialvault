use std::sync::Arc;
use uuid::Uuid;

use crate::db::{Database, ProcessJob};
use crate::error::{AppError, AppResult};

pub struct ProcessService {
    db: Arc<Database>,
}

impl ProcessService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub async fn create_job(
        &self,
        username: &str,
        process_id: &str,
        inputs: &serde_json::Value,
    ) -> AppResult<Uuid> {
        let job_id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO spatialvault.processes_jobs
            (id, process_id, owner, inputs)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(job_id)
        .bind(process_id)
        .bind(username)
        .bind(inputs)
        .execute(self.db.pool())
        .await?;

        // In a full implementation, we would also:
        // 1. Enqueue the job for background processing
        // 2. Notify the worker process

        Ok(job_id)
    }

    pub async fn list_jobs(&self, username: &str) -> AppResult<Vec<ProcessJob>> {
        let jobs: Vec<ProcessJob> = sqlx::query_as(
            r#"
            SELECT * FROM spatialvault.processes_jobs
            WHERE owner = $1
            ORDER BY created DESC
            LIMIT 100
            "#,
        )
        .bind(username)
        .fetch_all(self.db.pool())
        .await?;

        Ok(jobs)
    }

    pub async fn get_job(&self, username: &str, job_id: Uuid) -> AppResult<Option<ProcessJob>> {
        let job: Option<ProcessJob> = sqlx::query_as(
            "SELECT * FROM spatialvault.processes_jobs WHERE id = $1 AND owner = $2",
        )
        .bind(job_id)
        .bind(username)
        .fetch_optional(self.db.pool())
        .await?;

        Ok(job)
    }

    pub async fn update_job_status(
        &self,
        job_id: Uuid,
        status: &str,
        message: Option<&str>,
        progress: Option<i32>,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE spatialvault.processes_jobs
            SET
                status = $2,
                message = COALESCE($3, message),
                progress = COALESCE($4, progress),
                updated = NOW(),
                started = CASE WHEN $2 = 'running' AND started IS NULL THEN NOW() ELSE started END,
                finished = CASE WHEN $2 IN ('successful', 'failed', 'dismissed') THEN NOW() ELSE finished END
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .bind(status)
        .bind(message)
        .bind(progress)
        .execute(self.db.pool())
        .await?;

        Ok(())
    }

    pub async fn set_job_outputs(
        &self,
        job_id: Uuid,
        outputs: &serde_json::Value,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE spatialvault.processes_jobs
            SET outputs = $2, status = 'successful', finished = NOW(), updated = NOW()
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .bind(outputs)
        .execute(self.db.pool())
        .await?;

        Ok(())
    }

    pub async fn dismiss_job(&self, username: &str, job_id: Uuid) -> AppResult<()> {
        let result = sqlx::query(
            r#"
            UPDATE spatialvault.processes_jobs
            SET status = 'dismissed', finished = NOW(), updated = NOW()
            WHERE id = $1 AND owner = $2 AND status IN ('accepted', 'running')
            "#,
        )
        .bind(job_id)
        .bind(username)
        .execute(self.db.pool())
        .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(
                "Job not found or cannot be dismissed".to_string(),
            ));
        }

        Ok(())
    }
}
