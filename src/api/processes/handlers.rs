use aide::{
    axum::{
        ApiRouter,
        routing::{delete_with, get_with, post_with},
    },
    transform::TransformOperation,
};
use axum::{
    Json,
    extract::{Extension, State},
    http::{HeaderMap, StatusCode, header},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use super::{import_pointcloud, import_raster};
use crate::api::common::{Link, media_type, rel};
use crate::auth::AuthenticatedUser;
use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::services::ProcessService;

/// Process summary
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSummary {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub version: String,
    pub job_control_options: Vec<String>,
    pub links: Vec<Link>,
}

/// Process list response
#[derive(Debug, Serialize, JsonSchema)]
pub struct ProcessList {
    pub processes: Vec<ProcessSummary>,
    pub links: Vec<Link>,
}

/// Job status response (OGC API Processes)
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobStatusResponse {
    pub job_id: String,
    pub process_id: String,
    pub status: String,
    #[serde(rename = "type")]
    pub job_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    pub links: Vec<Link>,
}

/// Job list response
#[derive(Debug, Serialize, JsonSchema)]
pub struct JobList {
    pub jobs: Vec<JobStatusResponse>,
    pub links: Vec<Link>,
}

/// Execute request for import-raster process
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExecuteImportRaster {
    pub inputs: import_raster::ImportRasterInputs,
}

/// Execute request for import-pointcloud process
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExecuteImportPointCloud {
    pub inputs: import_pointcloud::ImportPointCloudInputs,
}

/// List available processes
pub async fn list_processes(Extension(config): Extension<Arc<Config>>) -> Json<ProcessList> {
    let base_url = &config.base_url;

    let processes = vec![
        ProcessSummary {
            id: import_raster::PROCESS_ID.to_string(),
            title: "Import Raster".to_string(),
            description: Some("Import a raster file into a collection".to_string()),
            version: "1.0.0".to_string(),
            job_control_options: vec!["async-execute".to_string()],
            links: vec![
                Link::new(
                    format!("{}/processes/{}", base_url, import_raster::PROCESS_ID),
                    rel::SELF,
                )
                .with_type(media_type::JSON),
            ],
        },
        ProcessSummary {
            id: import_pointcloud::PROCESS_ID.to_string(),
            title: "Import Point Cloud".to_string(),
            description: Some("Import a point cloud file into a collection".to_string()),
            version: "1.0.0".to_string(),
            job_control_options: vec!["async-execute".to_string()],
            links: vec![
                Link::new(
                    format!("{}/processes/{}", base_url, import_pointcloud::PROCESS_ID),
                    rel::SELF,
                )
                .with_type(media_type::JSON),
            ],
        },
    ];

    Json(ProcessList {
        processes,
        links: vec![
            Link::new(format!("{}/processes", base_url), rel::SELF).with_type(media_type::JSON),
        ],
    })
}

fn list_processes_docs(op: TransformOperation) -> TransformOperation {
    op.summary("List processes")
        .description("Returns the list of available processing operations")
        .tag("Processes")
        .response_with::<200, Json<ProcessList>, _>(|res| {
            res.description("List of available processes")
        })
}

/// Path parameters for single process endpoint
#[aide::axum::typed_path]
#[typed_path("/processes/{process_id}")]
pub struct ProcessPath {
    /// The process identifier (e.g., "import-raster")
    pub process_id: String,
}

/// Get process description
pub async fn get_process(
    Extension(_config): Extension<Arc<Config>>,
    path: ProcessPath,
) -> AppResult<Json<serde_json::Value>> {
    let process_id = path.process_id;
    let description = match process_id.as_str() {
        "import-raster" => import_raster::process_description(),
        "import-pointcloud" => import_pointcloud::process_description(),
        _ => {
            return Err(AppError::NotFound(format!(
                "Process not found: {}",
                process_id
            )));
        }
    };

    Ok(Json(description))
}

fn get_process_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get process description")
        .description("Returns the detailed description of a processing operation, including inputs and outputs")
        .tag("Processes")
        .response_with::<200, Json<serde_json::Value>, _>(|res| {
            res.description("Process description")
        })
        .response_with::<404, (), _>(|res| res.description("Process not found"))
}

/// Helper to create job response
fn create_job_response(
    job_id: Uuid,
    process_id: &str,
    base_url: &str,
) -> (StatusCode, HeaderMap, Json<JobStatusResponse>) {
    let response = JobStatusResponse {
        job_id: job_id.to_string(),
        process_id: process_id.to_string(),
        status: "accepted".to_string(),
        job_type: "process".to_string(),
        message: Some("Job queued for processing".to_string()),
        progress: Some(0),
        created: Some(chrono::Utc::now().to_rfc3339()),
        started: None,
        finished: None,
        updated: Some(chrono::Utc::now().to_rfc3339()),
        links: vec![
            Link::new(format!("{}/jobs/{}", base_url, job_id), rel::SELF)
                .with_type(media_type::JSON),
            Link::new(
                format!("{}/jobs/{}/results", base_url, job_id),
                "http://www.opengis.net/def/rel/ogc/1.0/results",
            )
            .with_type(media_type::JSON),
        ],
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::LOCATION,
        format!("{}/jobs/{}", base_url, job_id).parse().unwrap(),
    );

    (StatusCode::CREATED, headers, Json(response))
}

/// Execute import-raster process
pub async fn execute_import_raster(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<ProcessService>>,
    Json(request): Json<ExecuteImportRaster>,
) -> AppResult<(StatusCode, HeaderMap, Json<JobStatusResponse>)> {
    // Validate inputs
    request.inputs.validate()?;

    // Create job with inputs serialized to JSON
    let inputs_json = serde_json::to_value(&request.inputs)?;
    let job_id = service
        .create_job(&user.username, import_raster::PROCESS_ID, &inputs_json)
        .await?;

    Ok(create_job_response(
        job_id,
        import_raster::PROCESS_ID,
        &config.base_url,
    ))
}

fn execute_import_raster_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Execute import-raster")
        .description("Imports a raster file into a collection. Accepts COG (pass-through) or other formats (converts to COG via GDAL).")
        .tag("Processes")
        .response_with::<201, Json<JobStatusResponse>, _>(|res| {
            res.description("Job created successfully")
        })
        .response_with::<400, (), _>(|res| res.description("Invalid inputs"))
}

/// Execute import-pointcloud process
pub async fn execute_import_pointcloud(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<ProcessService>>,
    Json(request): Json<ExecuteImportPointCloud>,
) -> AppResult<(StatusCode, HeaderMap, Json<JobStatusResponse>)> {
    // Validate inputs
    request.inputs.validate()?;

    // Create job with inputs serialized to JSON
    let inputs_json = serde_json::to_value(&request.inputs)?;
    let job_id = service
        .create_job(&user.username, import_pointcloud::PROCESS_ID, &inputs_json)
        .await?;

    Ok(create_job_response(
        job_id,
        import_pointcloud::PROCESS_ID,
        &config.base_url,
    ))
}

fn execute_import_pointcloud_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Execute import-pointcloud")
        .description("Imports a point cloud file into a collection. Accepts COPC (pass-through) or other formats (converts to COPC).")
        .tag("Processes")
        .response_with::<201, Json<JobStatusResponse>, _>(|res| {
            res.description("Job created successfully")
        })
        .response_with::<400, (), _>(|res| res.description("Invalid inputs"))
}

/// List jobs
pub async fn list_jobs(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<ProcessService>>,
) -> AppResult<Json<JobList>> {
    let jobs = service.list_jobs(&user.username).await?;

    let base_url = &config.base_url;

    let job_responses: Vec<JobStatusResponse> = jobs
        .into_iter()
        .map(|job| JobStatusResponse {
            job_id: job.id.to_string(),
            process_id: job.process_id,
            status: job.status,
            job_type: job.job_type.unwrap_or_else(|| "process".to_string()),
            message: job.message,
            progress: job.progress,
            created: job.created.map(|dt| dt.to_rfc3339()),
            started: job.started.map(|dt| dt.to_rfc3339()),
            finished: job.finished.map(|dt| dt.to_rfc3339()),
            updated: job.updated.map(|dt| dt.to_rfc3339()),
            links: vec![
                Link::new(format!("{}/jobs/{}", base_url, job.id), rel::SELF)
                    .with_type(media_type::JSON),
            ],
        })
        .collect();

    Ok(Json(JobList {
        jobs: job_responses,
        links: vec![Link::new(format!("{}/jobs", base_url), rel::SELF).with_type(media_type::JSON)],
    }))
}

fn list_jobs_docs(op: TransformOperation) -> TransformOperation {
    op.summary("List jobs")
        .description("Returns a list of all jobs owned by the authenticated user")
        .tag("Processes")
        .response_with::<200, Json<JobList>, _>(|res| res.description("List of jobs"))
}

/// Path parameters for single job endpoint
#[aide::axum::typed_path]
#[typed_path("/jobs/{job_id}")]
pub struct JobPath {
    /// The job UUID
    pub job_id: Uuid,
}

/// Get job status
pub async fn get_job(
    Extension(config): Extension<Arc<Config>>,
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<ProcessService>>,
    path: JobPath,
) -> AppResult<Json<JobStatusResponse>> {
    let job_id = path.job_id;
    let job = service
        .get_job(&user.username, job_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Job not found: {}", job_id)))?;

    let base_url = &config.base_url;

    let response = JobStatusResponse {
        job_id: job.id.to_string(),
        process_id: job.process_id,
        status: job.status.clone(),
        job_type: job.job_type.unwrap_or_else(|| "process".to_string()),
        message: job.message,
        progress: job.progress,
        created: job.created.map(|dt| dt.to_rfc3339()),
        started: job.started.map(|dt| dt.to_rfc3339()),
        finished: job.finished.map(|dt| dt.to_rfc3339()),
        updated: job.updated.map(|dt| dt.to_rfc3339()),
        links: vec![
            Link::new(format!("{}/jobs/{}", base_url, job_id), rel::SELF)
                .with_type(media_type::JSON),
            Link::new(
                format!("{}/jobs/{}/results", base_url, job_id),
                "http://www.opengis.net/def/rel/ogc/1.0/results",
            )
            .with_type(media_type::JSON),
        ],
    };

    Ok(Json(response))
}

fn get_job_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get job status")
        .description("Returns the current status of a job")
        .tag("Processes")
        .response_with::<200, Json<JobStatusResponse>, _>(|res| res.description("Job status"))
        .response_with::<404, (), _>(|res| res.description("Job not found"))
}

/// Path parameters for job results endpoint
#[aide::axum::typed_path]
#[typed_path("/jobs/{job_id}/results")]
pub struct JobResultsPath {
    /// The job UUID
    pub job_id: Uuid,
}

/// Get job results
pub async fn get_job_results(
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<ProcessService>>,
    path: JobResultsPath,
) -> AppResult<Json<serde_json::Value>> {
    let job_id = path.job_id;
    let job = service
        .get_job(&user.username, job_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Job not found: {}", job_id)))?;

    // Check if job is complete
    if job.status != "successful" {
        return Err(AppError::BadRequest(format!(
            "Job is not complete. Status: {}",
            job.status
        )));
    }

    let outputs = job
        .outputs
        .ok_or_else(|| AppError::Internal("Job completed but no outputs".to_string()))?;

    Ok(Json(outputs))
}

fn get_job_results_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get job results")
        .description("Returns the results of a completed job")
        .tag("Processes")
        .response_with::<200, Json<serde_json::Value>, _>(|res| res.description("Job results"))
        .response_with::<400, (), _>(|res| res.description("Job not yet complete"))
        .response_with::<404, (), _>(|res| res.description("Job not found"))
}

/// Dismiss (cancel) a job
pub async fn dismiss_job(
    Extension(user): Extension<AuthenticatedUser>,
    State(service): State<Arc<ProcessService>>,
    path: JobPath,
) -> AppResult<StatusCode> {
    let job_id = path.job_id;
    service.dismiss_job(&user.username, job_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn dismiss_job_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Dismiss job")
        .description("Cancels or dismisses a job")
        .tag("Processes")
        .response_with::<204, (), _>(|res| res.description("Job dismissed"))
        .response_with::<404, (), _>(|res| res.description("Job not found"))
}

pub fn routes(service: Arc<ProcessService>) -> ApiRouter {
    ApiRouter::new()
        .api_route("/processes", get_with(list_processes, list_processes_docs))
        .api_route(
            "/processes/{process_id}",
            get_with(get_process, get_process_docs),
        )
        .api_route(
            "/processes/import-raster/execution",
            post_with(execute_import_raster, execute_import_raster_docs),
        )
        .api_route(
            "/processes/import-pointcloud/execution",
            post_with(execute_import_pointcloud, execute_import_pointcloud_docs),
        )
        .api_route("/jobs", get_with(list_jobs, list_jobs_docs))
        .api_route(
            "/jobs/{job_id}",
            get_with(get_job, get_job_docs).delete_with(dismiss_job, dismiss_job_docs),
        )
        .api_route(
            "/jobs/{job_id}/results",
            get_with(get_job_results, get_job_results_docs),
        )
        .with_state(service)
}
