//! HTTP API 层(路由 → 服务 → 仓储)。
//!
//! | 端点 | 说明 |
//! |---|---|
//! | `GET /api/health` | 健康检查 |
//! | `GET /api/archives` | 列出数据目录中的 .xp3 |
//! | `GET /api/archives/:name/files` | 归档条目列表(懒打开索引) |
//! | `GET /api/archives/:name/file?path=…` | 原始提取单个文件(懒解压) |
//! | `GET /api/archives/:name/scn?path=…` | 反编译 SCN 剧本为文本 |
//! | `GET /api/archives/:name/img?path=…` | 解码图像(TLG/PSB→PNG,WebP 透传) |
//! | 其它 | 静态服务 web 目录 |

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path as AxPath, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

use crate::decode;
use crate::repo::ArchiveRepo;

pub struct AppState {
    pub repo: Arc<ArchiveRepo>,
    pub web_dir: PathBuf,
}

pub fn router(state: Arc<AppState>) -> Router {
    let serve = ServeDir::new(&state.web_dir).append_index_html_on_directories(true);
    // 静态 web 一律 no-cache:每次重新校验,避免旧 index.html/main.js 版本错配
    // (新 JS 引用旧 HTML 缺失的 #id → 启动期 addEventListener 于 null)。
    let no_cache = SetResponseHeaderLayer::overriding(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache"),
    );
    Router::new()
        .route("/api/health", get(health))
        .route("/api/version", get(version_manifest))
        .route("/api/archives", get(list_archives))
        .route("/api/archives/:name/files", get(archive_files))
        .route("/api/archives/:name/file", get(archive_file))
        .route("/api/archives/:name/scn", get(archive_scn))
        .route("/api/archives/:name/img", get(archive_img))
        .fallback_service(serve)
        .layer(no_cache)
        .with_state(state)
}

// ---------- 处理器 ----------

async fn health() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") })),
    )
}

/// 版本清单(热更新):引擎/服务端版本 + 各归档的大小与 mtime 指纹。
async fn version_manifest(State(st): State<Arc<AppState>>) -> impl IntoResponse {
    let archives = st.repo.manifest();
    (StatusCode::OK, Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "archives": archives,
    })))
}

async fn list_archives(State(st): State<Arc<AppState>>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({ "archives": st.repo.list_archives() })),
    )
}

async fn archive_files(AxPath(name): AxPath<String>, State(st): State<Arc<AppState>>) -> Response {
    match st.repo.entries(&name) {
        Ok(files) => (
            StatusCode::OK,
            Json(json!({ "archive": name, "count": files.len(), "files": files })),
        )
            .into_response(),
        Err(e) => err_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

#[derive(Deserialize)]
struct EntryQuery {
    path: String,
}

async fn archive_file(
    AxPath(name): AxPath<String>,
    Query(q): Query<EntryQuery>,
    State(st): State<Arc<AppState>>,
) -> Response {
    match st.repo.read(&name, &q.path) {
        Ok(bytes) => {
            let mime = decode::content_type(&q.path, &bytes);
            bytes_response(bytes, mime)
        }
        Err(e) => err_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

async fn archive_scn(
    AxPath(name): AxPath<String>,
    Query(q): Query<EntryQuery>,
    State(st): State<Arc<AppState>>,
) -> Response {
    let out = st.repo.read(&name, &q.path).and_then(|bytes| {
        decode::decompile_scn(&bytes).map_err(|e| anyhow::anyhow!("反编译失败: {e}"))
    });
    match out {
        Ok(text) => bytes_response(text.into_bytes(), "text/plain; charset=utf-8"),
        Err(e) => err_response(StatusCode::UNPROCESSABLE_ENTITY, &e.to_string()),
    }
}

async fn archive_img(
    AxPath(name): AxPath<String>,
    Query(q): Query<EntryQuery>,
    State(st): State<Arc<AppState>>,
) -> Response {
    let out = st.repo.read(&name, &q.path).and_then(|bytes| {
        decode::decode_image(&bytes).map_err(|e| anyhow::anyhow!("解码失败: {e}"))
    });
    match out {
        Ok((bytes, mime)) => bytes_response(bytes, mime),
        Err(e) => err_response(StatusCode::UNPROCESSABLE_ENTITY, &e.to_string()),
    }
}

// ---------- 响应工具 ----------

fn err_response(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

fn bytes_response(bytes: Vec<u8>, mime: &str) -> Response {
    let mut res = Response::new(Body::from(bytes));
    if let Ok(v) = HeaderValue::from_str(mime) {
        res.headers_mut().insert(header::CONTENT_TYPE, v);
    }
    res
}
