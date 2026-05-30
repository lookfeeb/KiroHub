mod compress;
mod converter;
mod eventstream;
pub(crate) mod load_balancer;
pub mod log_store;
mod models;
pub mod prompt_cache;
pub mod prompt_filter;
mod proxy;
pub mod response_cache;
mod stream;
mod thinking_parser;
mod token_cache;
mod token_estimator;
mod config;
mod request_log;
mod stats;
mod paths;
pub(crate) use config::*;
pub(crate) use request_log::*;
pub(crate) use stats::*;
pub(crate) use paths::*;

use axum::{
    extract::{ConnectInfo, State},
    http::HeaderMap,
    response::{Json, Response},
    routing::{get, post},
    Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};
use tauri::{AppHandle, Manager};
use tokio::{
    net::TcpListener,
    sync::{oneshot, Mutex as AsyncMutex},
    task::JoinHandle,
};

use crate::clients::http_client::{build_streaming_http_client, is_supported_kiro_region};
use crate::gateway::token_cache::TokenCache;

#[cfg(test)]
use std::io::{BufRead, BufReader, Write};
#[cfg(test)]
use std::path::Path;

#[cfg(test)]
thread_local! {
    static REQUEST_LOG_PATH_OVERRIDE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn request_log_path_override() -> Option<PathBuf> {
    REQUEST_LOG_PATH_OVERRIDE.with(|cell| cell.borrow().clone())
}

#[cfg(test)]
fn set_request_log_path_override(path: Option<PathBuf>) {
    REQUEST_LOG_PATH_OVERRIDE.with(|cell| *cell.borrow_mut() = path);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub client_api_keys: Vec<String>,
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default = "default_account_mode")]
    pub account_mode: String,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default = "default_threshold")]
    pub threshold: i32,
    #[serde(default = "default_local_only")]
    pub local_only: bool,
    #[serde(default)]
    pub allowed_ips: Vec<String>,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub model_mappings: Vec<ModelMappingRule>,
    /// 系统提示过滤：检测 Claude Code 系统提示并替换为精简版
    #[serde(default)]
    pub filter_claude_code: bool,
    /// 系统提示过滤：去掉 --- SYSTEM PROMPT --- 边界标记
    #[serde(default)]
    pub filter_strip_boundaries: bool,
    /// 系统提示过滤：去掉环境噪音行（git status、recent commits 等）
    #[serde(default)]
    pub filter_env_noise: bool,
    /// 自定义提示过滤规则
    #[serde(default)]
    pub prompt_filter_rules: Vec<PromptFilterRule>,
    /// 是否记录请求日志
    #[serde(default = "default_true_val")]
    pub log_requests: bool,
    /// 响应缓存：是否启用
    #[serde(default = "default_true_val")]
    pub response_cache_enabled: bool,
    /// 响应缓存：TTL（秒）
    #[serde(default = "default_cache_ttl")]
    pub response_cache_ttl: u64,
}

fn default_cache_ttl() -> u64 { 180 }

/// 自定义提示过滤规则
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptFilterRule {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true_val")]
    pub enabled: bool,
    /// regex | lines-containing
    pub rule_type: String,
    /// 匹配模式（正则表达式或子串）
    pub match_pattern: String,
    /// 替换内容（仅 regex 类型使用，空 = 删除匹配）
    #[serde(default)]
    pub replace: String,
}

/// 模型映射规则
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelMappingRule {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true_val")]
    pub enabled: bool,
    /// replace | alias | loadbalance
    #[serde(default = "default_mapping_type")]
    pub rule_type: String,
    pub source_model: String,
    pub target_models: Vec<String>,
    #[serde(default)]
    pub weights: Vec<u32>,
}

fn default_true_val() -> bool { true }
fn default_mapping_type() -> String { "replace".to_string() }


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayStatus {
    pub running: bool,
    pub host: String,
    pub port: u16,
    pub request_count: u64,
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_config: Option<GatewayConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayRequestLogEntry {
    pub occurred_at: String,
    pub request_index: u64,
    pub endpoint: String,
    pub client_ip: String,
    pub model: Option<String>,
    pub stream: bool,
    pub upstream_source: Option<String>,
    pub region: Option<String>,
    pub status_code: u16,
    pub outcome: String,
    pub duration_ms: u64,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_body: Option<String>,
    /// Prompt Caching: 输入 tokens
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<i32>,
    /// Prompt Caching: 输出 tokens
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<i32>,
    /// Prompt Caching: 缓存读取 tokens（节省 90% 成本）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<i32>,
    /// Prompt Caching: 缓存写入 tokens（首次写入成本 +25%）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<i32>,
    /// 错误类型（如 invalid_request_error, authentication_error 等）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayRequestStats {
    pub total: usize,
    pub success: usize,
    pub error: usize,
    pub streaming: usize,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cache_read_tokens: i64,
    pub total_cache_creation_tokens: i64,
    pub requests_with_cache: usize,
    pub max_duration_ms: u64,
    pub avg_duration_ms: u64,
}

#[derive(Debug)]
pub struct GatewayRuntime {
    pub config: GatewayConfig,
    pub request_count: Arc<AtomicU64>,
    pub last_error: Arc<AsyncMutex<Option<String>>>,
    pub log_store: Arc<log_store::LogStore>,
    pub response_cache: Arc<AsyncMutex<response_cache::ResponseCache>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_task: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResponsesSessionEntry {
    #[allow(dead_code)]
    pub response_id: String,
    pub previous_response_id: Option<String>,
    pub request_messages: Vec<crate::gateway::models::NormalizedMessage>,
    pub request_tools: Option<Vec<crate::gateway::models::Tool>>,
    pub request_tool_choice: Option<serde_json::Value>,
    pub response_text: String,
    pub tool_calls: Vec<(String, String, String)>,
    pub updated_at: Instant,
}

pub(crate) type ResponsesSessionStore = Arc<AsyncMutex<HashMap<String, ResponsesSessionEntry>>>;

#[derive(Clone)]
struct RouterState {
    config: GatewayConfig,
    request_count: Arc<AtomicU64>,
    last_error: Arc<AsyncMutex<Option<String>>>,
    http: Client,
    responses_sessions: ResponsesSessionStore,
    #[allow(dead_code)]
    token_cache: Arc<AsyncMutex<TokenCache>>,
    load_balancer: Arc<load_balancer::LoadBalancer>,
    log_store: Arc<log_store::LogStore>,
    response_cache: Arc<AsyncMutex<response_cache::ResponseCache>>,
}

#[derive(Debug, Clone, Copy)]
enum ResponseFormat {
    Anthropic,
    Responses,
    OpenAI,
}

const CONFIG_DIR: &str = ".kirohub";
const CONFIG_FILE: &str = "gateway-config.json";
const LOGS_DIR: &str = "logs";
#[cfg(test)]
const REQUEST_LOG_FILE: &str = "gateway-request-log.jsonl";
const DEFAULT_AGENT_MODE: &str = "q-developer-converse";

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8765
}

fn default_region() -> String {
    "us-east-1".to_string()
}

fn default_account_mode() -> String {
    "pool".to_string()
}

fn default_strategy() -> String {
    "round_robin".to_string()
}

fn default_threshold() -> i32 {
    90
}

fn default_local_only() -> bool {
    true
}

fn default_log_level() -> String {
    "debug".to_string()
}


impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: default_host(),
            port: default_port(),
            access_token: None,
            client_api_keys: Vec::new(),
            region: default_region(),
            account_mode: default_account_mode(),
            account_id: None,
            group_id: None,
            strategy: default_strategy(),
            threshold: default_threshold(),
            local_only: default_local_only(),
            allowed_ips: Vec::new(),
            log_level: default_log_level(),
            model_mappings: Vec::new(),
            filter_claude_code: false,
            filter_strip_boundaries: false,
            filter_env_noise: false,
            prompt_filter_rules: Vec::new(),
            log_requests: true,
            response_cache_enabled: true,
            response_cache_ttl: default_cache_ttl(),
        }
    }
}


impl GatewayStatus {
    pub fn stopped(config: &GatewayConfig) -> Self {
        Self {
            running: false,
            host: config.host.clone(),
            port: config.port,
            request_count: 0,
            last_error: None,
            runtime_config: None,
        }
    }
}









// ===== 请求日志：SQLite 持久化（§9.1 异步批量写）=====

static REQUEST_LOG_TX: std::sync::OnceLock<
    tokio::sync::mpsc::UnboundedSender<GatewayRequestLogEntry>,
> = std::sync::OnceLock::new();





















pub async fn start_gateway(
    state: &tauri::State<'_, crate::state::AppState>,
    config: GatewayConfig,
) -> Result<GatewayStatus, String> {
    let config = normalize_config(&config);
    ensure_config_valid(&config)?;

    let existing = {
        let mut guard = state
            .gateway
            .lock()
            .map_err(|_| "获取 gateway 状态失败".to_string())?;
        guard.take()
    };

    if let Some(mut rt) = existing {
        stop_runtime(&mut rt).await;
    }

    let runtime = spawn_runtime(config.clone()).await?;
    let status = GatewayStatus {
        running: true,
        host: config.host.clone(),
        port: config.port,
        request_count: 0,
        last_error: None,
        runtime_config: Some(config.clone()),
    };

    let mut guard = state
        .gateway
        .lock()
        .map_err(|_| "获取 gateway 状态失败".to_string())?;
    *guard = Some(runtime);

    Ok(status)
}

pub async fn stop_gateway(state: &tauri::State<'_, crate::state::AppState>) -> Result<(), String> {
    let existing = {
        let mut guard = state
            .gateway
            .lock()
            .map_err(|_| "获取 gateway 状态失败".to_string())?;
        guard.take()
    };

    if let Some(mut rt) = existing {
        stop_runtime(&mut rt).await;
    }

    Ok(())
}

pub async fn get_gateway_status(
    state: &tauri::State<'_, crate::state::AppState>,
) -> Result<GatewayStatus, String> {
    let snapshot = {
        let guard = state
            .gateway
            .lock()
            .map_err(|_| "获取 gateway 状态失败".to_string())?;

        guard.as_ref().map(|rt| {
            (
                rt.config.clone(),
                rt.request_count.load(Ordering::Relaxed),
                rt.last_error.clone(),
                rt.server_task.is_some(),
            )
        })
    };

    if let Some((config, request_count, last_error, running)) = snapshot {
        let last_error_text = last_error.lock().await.clone();
        Ok(GatewayStatus {
            running,
            host: config.host.clone(),
            port: config.port,
            request_count,
            last_error: last_error_text,
            runtime_config: Some(config),
        })
    } else {
        let cfg = load_gateway_config().unwrap_or_default();
        Ok(GatewayStatus::stopped(&cfg))
    }
}

fn router(state: RouterState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/v1/models", get(models_handler))
        .route("/v1/messages", post(messages_handler))
        .route("/v1/messages/count_tokens", post(count_tokens_handler))
        .route("/v1/responses", post(responses_handler))
        .route("/v1/chat/completions", post(openai_chat_handler))
        .with_state(state)
}

async fn spawn_runtime(config: GatewayConfig) -> Result<GatewayRuntime, String> {
    ensure_config_valid(&config)?;

    let request_count = Arc::new(AtomicU64::new(0));
    let last_error = Arc::new(AsyncMutex::new(None));
    let responses_sessions = Arc::new(AsyncMutex::new(HashMap::new()));
    let token_cache = Arc::new(AsyncMutex::new(TokenCache::new()));

    let http = build_streaming_http_client()
        .map_err(|e| format!("初始化 HTTP 客户端失败: {e}"))?;

    // 初始化负载均衡器
    let strategy = load_balancer::LoadBalancerStrategy::from_str(&config.strategy);
    let load_balancer = Arc::new(load_balancer::LoadBalancer::new(strategy));

    // 初始化内存日志存储（保存最近 10000 条日志）
    let log_store = Arc::new(log_store::LogStore::new(10000));

    // 从 SQLite 加载历史日志到内存（启动时恢复）
    if let Ok(history) = get_gateway_request_logs_from_db(Some(500)) {
        let store_clone = log_store.clone();
        tokio::spawn(async move {
            // 历史日志是倒序的（最新在前），需要反转后逐条添加
            for entry in history.into_iter().rev() {
                store_clone.add(entry).await;
            }
        });
    }

    // 初始化响应缓存
    let cache_config = response_cache::CacheConfig {
        summary_cache_enabled: config.response_cache_enabled,
        summary_cache_max_age_seconds: config.response_cache_ttl,
        ..response_cache::CacheConfig::default()
    };
    let cache_dir = dirs::data_dir()
        .map(|p| p.join(".kirohub").join("cache"));
    let response_cache = Arc::new(AsyncMutex::new(response_cache::ResponseCache::new(
        cache_config,
        cache_dir,
    )));

    let state = RouterState {
        config: config.clone(),
        request_count: request_count.clone(),
        last_error: last_error.clone(),
        http,
        responses_sessions,
        token_cache,
        load_balancer,
        log_store: log_store.clone(),
        response_cache: response_cache.clone(),
    };

    let app = router(state);
    let addr = build_bind_addr(&config.host, config.port)?;

    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| format!("绑定端口失败: {e}"))?;

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = shutdown_rx.await;
    });

    let server_task = tokio::spawn(async move {
        if let Err(e) = server.await {
            log::error!("网关服务器错误: {e}");
        }
    });

    Ok(GatewayRuntime {
        config,
        request_count,
        last_error,
        log_store,
        response_cache,
        shutdown_tx: Some(shutdown_tx),
        server_task: Some(server_task),
    })
}

async fn stop_runtime(runtime: &mut GatewayRuntime) {
    if let Some(tx) = runtime.shutdown_tx.take() {
        let _ = tx.send(());
    }
    if let Some(task) = runtime.server_task.take() {
        let _ = task.await;
    }
}

pub async fn auto_start_if_enabled(app: &AppHandle) -> Result<(), String> {
    let cfg = load_gateway_config()?;
    if !cfg.enabled {
        return Ok(());
    }

    let state = app.state::<crate::state::AppState>();
    let _ = start_gateway(&state, cfg).await?;
    Ok(())
}




async fn health_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<RouterState>,
    headers: HeaderMap,
) -> Response {
    proxy::health_handler(state, addr, headers).await
}

async fn models_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<RouterState>,
    headers: HeaderMap,
) -> Response {
    proxy::models_handler(state, addr, headers).await
}

async fn count_tokens_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<RouterState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    proxy::count_tokens_handler(state, addr, headers, payload).await
}

async fn messages_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<RouterState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    proxy::proxy_handler(state, addr, headers, payload, ResponseFormat::Anthropic).await
}

async fn responses_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<RouterState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    proxy::proxy_handler(state, addr, headers, payload, ResponseFormat::Responses).await
}

async fn openai_chat_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<RouterState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    proxy::proxy_handler(state, addr, headers, payload, ResponseFormat::OpenAI).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{header::AUTHORIZATION, HeaderMap, HeaderValue, Method, Request, StatusCode};
    use serde_json::json;
    use std::{
        process,
        sync::{
            atomic::{AtomicU64, Ordering},
            Mutex,
        },
    };
    use tower::util::ServiceExt;

    static REQUEST_LOG_TEST_MUTEX: Mutex<()> = Mutex::new(());
    static REQUEST_LOG_TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct RequestLogTestFixture {
        path: PathBuf,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl RequestLogTestFixture {
        fn new() -> Self {
            let guard = REQUEST_LOG_TEST_MUTEX
                .lock()
                .expect("request log test mutex should lock");
            let dir = std::env::temp_dir().join(format!(
                "kiro-gateway-request-log-test-{}-{}",
                process::id(),
                REQUEST_LOG_TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).expect("request log test dir should create");
            let path = dir.join(REQUEST_LOG_FILE);
            set_request_log_path_override(Some(path.clone()));
            Self {
                path,
                _guard: guard,
            }
        }
    }

    impl Drop for RequestLogTestFixture {
        fn drop(&mut self) {
            set_request_log_path_override(None);
            if let Some(dir) = self.path.parent() {
                let _ = fs::remove_dir_all(dir);
            }
        }
    }

    fn gateway_runtime_test_state() -> RouterState {
        let config = GatewayConfig {
            access_token: Some("sk-test".to_string()),
            account_mode: "single".to_string(),
            account_id: Some("test-account".to_string()),
            ..GatewayConfig::default()
        };
        let strategy = load_balancer::LoadBalancerStrategy::from_str(&config.strategy);
        RouterState {
            config,
            request_count: Arc::new(AtomicU64::new(0)),
            last_error: Arc::new(AsyncMutex::new(None)),
            http: Client::new(),
            responses_sessions: Arc::new(AsyncMutex::new(HashMap::new())),
            token_cache: Arc::new(AsyncMutex::new(TokenCache::new())),
            load_balancer: Arc::new(load_balancer::LoadBalancer::new(strategy)),
            log_store: Arc::new(log_store::LogStore::new(1000)),
            response_cache: Arc::new(AsyncMutex::new(response_cache::ResponseCache::new(
                response_cache::CacheConfig::default(),
                None,
            ))),
        }
    }

    fn auth_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer sk-test"));
        headers
    }

    

    

    

    

    fn test_router_state() -> RouterState {
        let config = GatewayConfig::default();
        let strategy = load_balancer::LoadBalancerStrategy::from_str(&config.strategy);
        RouterState {
            config,
            request_count: Arc::new(AtomicU64::new(0)),
            last_error: Arc::new(AsyncMutex::new(None)),
            http: Client::new(),
            responses_sessions: Arc::new(AsyncMutex::new(HashMap::new())),
            token_cache: Arc::new(AsyncMutex::new(TokenCache::new())),
            load_balancer: Arc::new(load_balancer::LoadBalancer::new(strategy)),
            log_store: Arc::new(log_store::LogStore::new(1000)),
            response_cache: Arc::new(AsyncMutex::new(response_cache::ResponseCache::new(
                response_cache::CacheConfig::default(),
                None,
            ))),
        }
    }

    fn runtime_test_gateway_config(port: u16) -> GatewayConfig {
        GatewayConfig {
            port,
            local_only: false,
            allowed_ips: vec!["127.0.0.1".to_string()],
            account_mode: "single".to_string(),
            account_id: Some("test-account".to_string()),
            access_token: Some("sk-test".to_string()),
            ..GatewayConfig::default()
        }
    }

    #[tokio::test]
    async fn health_route_is_reachable() {
        let app = router(test_router_state());
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn responses_route_is_reachable() {
        let app = router(test_router_state());
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/responses")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "model": "claude-sonnet-4-5-20250929",
                            "input": [{ "role": "user", "content": "hello" }]
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn openai_chat_completions_endpoint_accepts_requests() {
        let app = router(test_router_state());
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/chat/completions")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "model": "claude-sonnet-4-5-20250929",
                            "messages": [{ "role": "user", "content": "hello" }]
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn lightweight_routes_increment_request_count_and_write_logs() {
        let fixture = RequestLogTestFixture::new();
        let state = gateway_runtime_test_state();
        let client_addr: SocketAddr = "127.0.0.1:4317".parse().expect("socket addr should parse");

        let health = proxy::health_handler(state.clone(), client_addr, auth_headers()).await;
        assert_eq!(health.status(), StatusCode::OK);

        let models = proxy::models_handler(state.clone(), client_addr, auth_headers()).await;
        assert_eq!(models.status(), StatusCode::OK);

        let count_tokens = proxy::count_tokens_handler(
            state.clone(),
            client_addr,
            auth_headers(),
            json!({
                "model": "claude-sonnet-4-5-20250929",
                "messages": [{ "role": "user", "content": "hello world" }]
            }),
        )
        .await;
        assert_eq!(count_tokens.status(), StatusCode::OK);

        assert_eq!(state.request_count.load(Ordering::Relaxed), 3);

        let logs = get_gateway_request_logs_from_path(fixture.path.as_path(), Some(10))
            .expect("request logs should read");
        assert_eq!(logs.len(), 3);
        assert_eq!(logs[0].endpoint, "count_tokens");
        assert_eq!(logs[1].endpoint, "models");
        assert_eq!(logs[2].endpoint, "health");
        assert_eq!(logs[0].status_code, 200);
        assert_eq!(logs[1].status_code, 200);
        assert_eq!(logs[2].status_code, 200);
        assert_eq!(logs[0].outcome, "success");
        assert_eq!(logs[1].outcome, "success");
        assert_eq!(logs[2].outcome, "success");
        assert_eq!(logs[0].client_ip, "127.0.0.1");
        assert!(
            logs[0].request_body.is_none(),
            "request body should not be logged by default"
        );
        assert!(
            logs[0].response_body.is_none(),
            "response body should not be logged by default"
        );
    }

    #[test]
    fn rejects_unsupported_region() {
        let config = GatewayConfig {
            region: "moon-east-1".to_string(),
            ..GatewayConfig::default()
        };

        let err = ensure_config_valid(&config).expect_err("unsupported region should fail");
        assert!(err.contains("region 不受支持"));
    }

    #[test]
    fn rejects_local_account_mode_for_gateway() {
        let config = GatewayConfig {
            account_mode: "local".to_string(),
            access_token: Some("sk-test".to_string()),
            ..GatewayConfig::default()
        };

        let err = ensure_config_valid(&config).expect_err("local mode should fail");
        assert!(
            err.contains("不再支持 local 模式"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn accepts_known_regions() {
        let mut config = GatewayConfig {
            account_id: Some("test-account".to_string()),
            access_token: Some("sk-test".to_string()),
            ..GatewayConfig::default()
        };
        for region in [
            "us-east-1",
            "eu-central-1",
            "us-west-2",
            "ap-northeast-1",
            "ap-southeast-1",
            "us-gov-west-1",
        ] {
            config.region = region.to_string();
            ensure_config_valid(&config).expect("known region should pass validation");
        }
    }

    #[test]
    fn rejects_remote_access_without_api_key() {
        let config = GatewayConfig {
            local_only: false,
            account_id: Some("test-account".to_string()),
            access_token: None,
            ..GatewayConfig::default()
        };

        let err =
            ensure_config_valid(&config).expect_err("remote access without api key should fail");
        assert!(err.contains("API Key"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_remote_access_without_allowlist() {
        let config = GatewayConfig {
            local_only: false,
            account_mode: "single".to_string(),
            account_id: Some("test-account".to_string()),
            access_token: Some("sk-test".to_string()),
            allowed_ips: Vec::new(),
            ..GatewayConfig::default()
        };

        let err =
            ensure_config_valid(&config).expect_err("remote access without allowlist should fail");
        assert!(err.contains("白名单"), "unexpected error: {err}");
    }

    #[test]
    fn normalize_config_promotes_legacy_access_token_to_client_api_keys() {
        let config = GatewayConfig {
            access_token: Some(" sk-primary ".to_string()),
            client_api_keys: Vec::new(),
            ..GatewayConfig::default()
        };

        let normalized = normalize_config(&config);

        assert_eq!(normalized.client_api_keys, vec!["sk-primary".to_string()]);
        assert_eq!(normalized.access_token.as_deref(), Some("sk-primary"));
    }

    #[test]
    fn normalize_config_deduplicates_client_api_keys() {
        let config = GatewayConfig {
            access_token: Some("sk-primary".to_string()),
            client_api_keys: vec![
                " sk-primary ".to_string(),
                "sk-secondary".to_string(),
                "".to_string(),
                "sk-secondary".to_string(),
            ],
            ..GatewayConfig::default()
        };

        let normalized = normalize_config(&config);

        assert_eq!(
            normalized.client_api_keys,
            vec!["sk-primary".to_string(), "sk-secondary".to_string()]
        );
    }

    #[test]
    fn rejects_config_without_any_client_api_keys() {
        let config = GatewayConfig {
            access_token: Some("   ".to_string()),
            client_api_keys: vec!["".to_string(), "   ".to_string()],
            account_mode: "single".to_string(),
            account_id: Some("test-account".to_string()),
            ..GatewayConfig::default()
        };

        let err = ensure_config_valid(&normalize_config(&config))
            .expect_err("missing client api keys should fail");
        assert!(err.contains("客户端 API Key"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn runtime_serves_health_over_real_http() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let port = listener
            .local_addr()
            .expect("local addr should resolve")
            .port();
        drop(listener);

        let config = runtime_test_gateway_config(port);
        let mut runtime = spawn_runtime(config).await.expect("runtime should start");

        let response = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{port}/health"))
            .bearer_auth("sk-test")
            .send()
            .await
            .expect("health request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        stop_runtime(&mut runtime).await;
    }

    #[tokio::test]
    async fn runtime_serves_models_over_real_http() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let port = listener
            .local_addr()
            .expect("local addr should resolve")
            .port();
        drop(listener);

        let config = runtime_test_gateway_config(port);
        let mut runtime = spawn_runtime(config).await.expect("runtime should start");

        let response = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{port}/v1/models"))
            .bearer_auth("sk-test")
            .send()
            .await
            .expect("models request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let payload: Value = response.json().await.expect("models response should be json");
        assert_eq!(payload.get("object").and_then(Value::as_str), Some("list"));
        assert!(
            payload
                .get("data")
                .and_then(Value::as_array)
                .is_some_and(|items| !items.is_empty()),
            "models response should include at least one model"
        );
        stop_runtime(&mut runtime).await;
    }

    #[tokio::test]
    async fn runtime_serves_count_tokens_over_real_http() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let port = listener
            .local_addr()
            .expect("local addr should resolve")
            .port();
        drop(listener);

        let config = runtime_test_gateway_config(port);
        let mut runtime = spawn_runtime(config).await.expect("runtime should start");

        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/v1/messages/count_tokens"))
            .bearer_auth("sk-test")
            .header("content-type", "application/json")
            .body(
                json!({
                    "model": "claude-sonnet-4-5-20250929",
                    "messages": [{ "role": "user", "content": "hello world" }]
                })
                .to_string(),
            )
            .send()
            .await
            .expect("count tokens request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let payload: Value = response
            .json()
            .await
            .expect("count tokens response should be json");
        assert!(
            payload
                .get("input_tokens")
                .and_then(Value::as_u64)
                .is_some_and(|n| n >= 2),
            "count_tokens 应返回正的 token 估算（对整个请求体保守估算）"
        );
        stop_runtime(&mut runtime).await;
    }

    #[tokio::test]
    async fn runtime_rejects_unauthenticated_health_requests_over_real_http() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let port = listener
            .local_addr()
            .expect("local addr should resolve")
            .port();
        drop(listener);

        let config = runtime_test_gateway_config(port);
        let mut runtime = spawn_runtime(config).await.expect("runtime should start");

        let response = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{port}/health"))
            .send()
            .await
            .expect("health request should succeed");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        stop_runtime(&mut runtime).await;
    }

    #[tokio::test]
    async fn runtime_rejects_raw_authorization_header_without_bearer_over_real_http() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let port = listener
            .local_addr()
            .expect("local addr should resolve")
            .port();
        drop(listener);

        let config = runtime_test_gateway_config(port);
        let mut runtime = spawn_runtime(config).await.expect("runtime should start");

        let response = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{port}/health"))
            .header("Authorization", "sk-test")
            .send()
            .await
            .expect("health request should succeed");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        stop_runtime(&mut runtime).await;
    }

    #[tokio::test]
    async fn runtime_rejects_unauthenticated_models_requests_over_real_http() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let port = listener
            .local_addr()
            .expect("local addr should resolve")
            .port();
        drop(listener);

        let config = runtime_test_gateway_config(port);
        let mut runtime = spawn_runtime(config).await.expect("runtime should start");

        let response = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{port}/v1/models"))
            .send()
            .await
            .expect("models request should succeed");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        stop_runtime(&mut runtime).await;
    }

    #[tokio::test]
    async fn runtime_rejects_unauthenticated_count_tokens_requests_over_real_http() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let port = listener
            .local_addr()
            .expect("local addr should resolve")
            .port();
        drop(listener);

        let config = runtime_test_gateway_config(port);
        let mut runtime = spawn_runtime(config).await.expect("runtime should start");

        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/v1/messages/count_tokens"))
            .header("content-type", "application/json")
            .body(
                json!({
                    "model": "claude-sonnet-4-5-20250929",
                    "input": [{ "role": "user", "content": "hello" }]
                })
                .to_string(),
            )
            .send()
            .await
            .expect("count tokens request should succeed");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        stop_runtime(&mut runtime).await;
    }

    #[tokio::test]
    async fn runtime_rejects_unauthenticated_responses_requests_over_real_http() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let port = listener
            .local_addr()
            .expect("local addr should resolve")
            .port();
        drop(listener);

        let config = runtime_test_gateway_config(port);
        let mut runtime = spawn_runtime(config).await.expect("runtime should start");

        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/v1/responses"))
            .header("content-type", "application/json")
            .body(
                json!({
                    "model": "claude-sonnet-4-5-20250929",
                    "input": [{ "role": "user", "content": "hello" }]
                })
                .to_string(),
            )
            .send()
            .await
            .expect("responses request should succeed");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        stop_runtime(&mut runtime).await;
    }

    #[tokio::test]
    async fn runtime_rejects_unauthenticated_messages_requests_over_real_http() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let port = listener
            .local_addr()
            .expect("local addr should resolve")
            .port();
        drop(listener);

        let config = runtime_test_gateway_config(port);
        let mut runtime = spawn_runtime(config).await.expect("runtime should start");

        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/v1/messages"))
            .header("content-type", "application/json")
            .body(
                json!({
                    "model": "claude-sonnet-4-5-20250929",
                    "messages": [{ "role": "user", "content": "hello" }]
                })
                .to_string(),
            )
            .send()
            .await
            .expect("messages request should succeed");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        stop_runtime(&mut runtime).await;
    }

    #[tokio::test]
    async fn runtime_requires_client_api_key_even_when_local_only() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let port = listener
            .local_addr()
            .expect("local addr should resolve")
            .port();
        drop(listener);

        let config = GatewayConfig {
            port,
            local_only: true,
            account_mode: "single".to_string(),
            account_id: Some("test-account".to_string()),
            access_token: Some("sk-test".to_string()),
            ..GatewayConfig::default()
        };
        let mut runtime = spawn_runtime(config).await.expect("runtime should start");

        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/v1/responses"))
            .header("content-type", "application/json")
            .body(
                json!({
                    "model": "claude-sonnet-4-5-20250929",
                    "input": [{ "role": "user", "content": "hello" }]
                })
                .to_string(),
            )
            .send()
            .await
            .expect("responses request should succeed");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        stop_runtime(&mut runtime).await;
    }
}
