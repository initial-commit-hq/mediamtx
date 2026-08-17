use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use time::format_description::well_known::Rfc3339;

use crate::config_view::ConfigViewError;
use crate::error::ApiError;
use crate::paginate;
use crate::state::AppState;
use crate::types::{
    EmptyListResponse, InfoResponse, OkResponse, PathConfListResponse, PathListResponse,
    RecordingResponse,
};

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(rename = "itemsPerPage")]
    items_per_page: Option<String>,
    page: Option<String>,
}

pub fn routes(state: AppState) -> Router {
    Router::new()
        .route("/v3/info", get(info))
        .route("/v3/paths/list", get(paths_list))
        .route("/v3/paths/get/{*name}", get(paths_get))
        .route("/v3/config/global/get", get(config_global_get))
        .route("/v3/config/global/patch", patch(config_global_patch))
        .route("/v3/config/pathdefaults/get", get(config_path_defaults_get))
        .route(
            "/v3/config/pathdefaults/patch",
            patch(config_path_defaults_patch),
        )
        .route("/v3/config/paths/list", get(config_paths_list))
        .route("/v3/config/paths/get/{*name}", get(config_paths_get))
        .route("/v3/config/paths/add/{*name}", post(config_paths_add))
        .route("/v3/config/paths/patch/{*name}", patch(config_paths_patch))
        .route(
            "/v3/config/paths/replace/{*name}",
            post(config_paths_replace),
        )
        .route(
            "/v3/config/paths/delete/{*name}",
            delete(config_paths_delete),
        )
        .route("/v3/auth/jwks/refresh", post(auth_jwks_refresh))
        .route("/v3/rtspconns/list", get(empty_list))
        .route("/v3/rtspconns/get/{id}", get(connection_not_found))
        .route("/v3/rtspsessions/list", get(empty_list))
        .route("/v3/rtspsessions/get/{id}", get(session_not_found))
        .route("/v3/rtspsessions/kick/{id}", post(session_not_found))
        .route("/v3/rtspsconns/list", get(empty_list))
        .route("/v3/rtspsconns/get/{id}", get(connection_not_found))
        .route("/v3/rtspssessions/list", get(empty_list))
        .route("/v3/rtspssessions/get/{id}", get(session_not_found))
        .route("/v3/rtspssessions/kick/{id}", post(session_not_found))
        .route("/v3/rtmpconns/list", get(empty_list))
        .route("/v3/rtmpconns/get/{id}", get(connection_not_found))
        .route("/v3/rtmpconns/kick/{id}", post(connection_not_found))
        .route("/v3/rtmpsconns/list", get(empty_list))
        .route("/v3/rtmpsconns/get/{id}", get(connection_not_found))
        .route("/v3/rtmpsconns/kick/{id}", post(connection_not_found))
        .route("/v3/webrtcsessions/list", get(empty_list))
        .route("/v3/webrtcsessions/get/{id}", get(session_not_found))
        .route("/v3/webrtcsessions/kick/{id}", post(session_not_found))
        .route("/v3/hlsmuxers/list", get(empty_list))
        .route("/v3/hlsmuxers/get/{*name}", get(muxer_not_found))
        .route("/v3/srtconns/list", get(empty_list))
        .route("/v3/srtconns/get/{id}", get(connection_not_found))
        .route("/v3/srtconns/kick/{id}", post(connection_not_found))
        .route("/v3/recordings/list", get(empty_list))
        .route("/v3/recordings/get/{*name}", get(recordings_get))
        .route(
            "/v3/recordings/deletesegment",
            delete(recordings_delete_segment),
        )
        .with_state(state)
}

async fn empty_list() -> Json<EmptyListResponse> {
    Json(EmptyListResponse::new())
}

async fn session_not_found(
    Path(id): Path<String>,
) -> Result<Json<OkResponse>, (axum::http::StatusCode, Json<ApiError>)> {
    let _ = id;
    Err(ApiError::not_found("session not found"))
}

async fn connection_not_found(
    Path(id): Path<String>,
) -> Result<Json<OkResponse>, (axum::http::StatusCode, Json<ApiError>)> {
    let _ = id;
    Err(ApiError::not_found("connection not found"))
}

async fn muxer_not_found(
    Path(name): Path<String>,
) -> Result<Json<OkResponse>, (axum::http::StatusCode, Json<ApiError>)> {
    let _ = name;
    Err(ApiError::not_found("muxer not found"))
}

async fn auth_jwks_refresh() -> Json<OkResponse> {
    // JWT auth is not wired yet; refresh is a no-op success matching OpenAPI `OK`.
    Json(OkResponse::new())
}

async fn recordings_get(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<RecordingResponse>, (axum::http::StatusCode, Json<ApiError>)> {
    validate_name(&name)?;
    if state.config_path(&name).await.is_none() {
        return Err(ApiError::not_found("path not found"));
    }
    Ok(Json(RecordingResponse {
        name,
        segments: Vec::new(),
    }))
}

#[derive(Debug, Deserialize)]
struct DeleteSegmentQuery {
    path: Option<String>,
    start: Option<String>,
}

async fn recordings_delete_segment(
    Query(query): Query<DeleteSegmentQuery>,
) -> Result<Json<OkResponse>, (axum::http::StatusCode, Json<ApiError>)> {
    let path = query.path.unwrap_or_default();
    if path.is_empty() {
        return Err(ApiError::bad_request("missing path"));
    }
    let start = query.start.unwrap_or_default();
    if start.is_empty() {
        return Err(ApiError::bad_request("invalid 'start' parameter"));
    }
    let _ = path;
    Err(ApiError::not_found("segment not found"))
}

async fn info(State(state): State<AppState>) -> Json<InfoResponse> {
    Json(InfoResponse {
        version: state.version().to_owned(),
        started: state
            .started()
            .format(&Rfc3339)
            .unwrap_or_else(|_| String::new()),
    })
}

async fn paths_list(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<PathListResponse>, (axum::http::StatusCode, Json<ApiError>)> {
    let all = state.paths().await;
    let (items, page_count, item_count) =
        paginate::paginate(&all, query.items_per_page.as_deref(), query.page.as_deref())
            .map_err(ApiError::bad_request)?;

    Ok(Json(PathListResponse {
        page_count,
        item_count,
        items,
    }))
}

async fn paths_get(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<crate::state::PathInfo>, (axum::http::StatusCode, Json<ApiError>)> {
    validate_name(&name)?;
    state
        .path(&name)
        .await
        .ok_or_else(|| ApiError::not_found(format!("path not found")))
        .map(Json)
}

async fn config_global_get(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(state.global_config().await)
}

async fn config_global_patch(
    State(state): State<AppState>,
    body: Result<Json<serde_json::Value>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<OkResponse>, (axum::http::StatusCode, Json<ApiError>)> {
    let Json(patch) = body.map_err(|err| ApiError::bad_request(err.to_string()))?;
    state.patch_global(patch).await.map_err(config_error)?;
    Ok(Json(OkResponse::new()))
}

async fn config_path_defaults_get(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(state.path_defaults_config().await)
}

async fn config_path_defaults_patch(
    State(state): State<AppState>,
    body: Result<Json<serde_json::Value>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<OkResponse>, (axum::http::StatusCode, Json<ApiError>)> {
    let Json(patch) = body.map_err(|err| ApiError::bad_request(err.to_string()))?;
    state
        .patch_path_defaults(patch)
        .await
        .map_err(config_error)?;
    Ok(Json(OkResponse::new()))
}

async fn config_paths_list(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<PathConfListResponse>, (axum::http::StatusCode, Json<ApiError>)> {
    let all = state.config_paths_list().await;

    let (items, page_count, item_count) =
        paginate::paginate(&all, query.items_per_page.as_deref(), query.page.as_deref())
            .map_err(ApiError::bad_request)?;

    Ok(Json(PathConfListResponse {
        page_count,
        item_count,
        items,
    }))
}

async fn config_paths_get(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<ApiError>)> {
    validate_name(&name)?;
    state
        .config_path(&name)
        .await
        .ok_or_else(|| ApiError::not_found("path configuration not found"))
        .map(Json)
}

async fn config_paths_add(
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Result<Json<serde_json::Value>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<OkResponse>, (axum::http::StatusCode, Json<ApiError>)> {
    validate_name(&name)?;
    let Json(body) = body.map_err(|err| ApiError::bad_request(err.to_string()))?;
    state
        .add_path_config(&name, body)
        .await
        .map_err(config_error)?;
    Ok(Json(OkResponse::new()))
}

async fn config_paths_patch(
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Result<Json<serde_json::Value>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<OkResponse>, (axum::http::StatusCode, Json<ApiError>)> {
    validate_name(&name)?;
    let Json(patch) = body.map_err(|err| ApiError::bad_request(err.to_string()))?;
    state
        .patch_path_config(&name, patch)
        .await
        .map_err(config_error)?;
    Ok(Json(OkResponse::new()))
}

async fn config_paths_replace(
    State(state): State<AppState>,
    Path(name): Path<String>,
    body: Result<Json<serde_json::Value>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<OkResponse>, (axum::http::StatusCode, Json<ApiError>)> {
    validate_name(&name)?;
    let Json(body) = body.map_err(|err| ApiError::bad_request(err.to_string()))?;
    state
        .replace_path_config(&name, body)
        .await
        .map_err(config_error)?;
    Ok(Json(OkResponse::new()))
}

async fn config_paths_delete(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<OkResponse>, (axum::http::StatusCode, Json<ApiError>)> {
    validate_name(&name)?;
    state
        .delete_path_config(&name)
        .await
        .map_err(config_error)?;
    Ok(Json(OkResponse::new()))
}

fn validate_name(name: &str) -> Result<(), (axum::http::StatusCode, Json<ApiError>)> {
    if name.is_empty() {
        return Err(ApiError::bad_request("invalid name"));
    }
    Ok(())
}

fn config_error(err: ConfigViewError) -> (axum::http::StatusCode, Json<ApiError>) {
    match err {
        ConfigViewError::PathNotFound => ApiError::not_found(err.to_string()),
        ConfigViewError::PathAlreadyExists | ConfigViewError::Invalid(_) => {
            ApiError::bad_request(err.to_string())
        }
    }
}
