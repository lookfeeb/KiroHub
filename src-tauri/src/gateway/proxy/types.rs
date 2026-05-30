use super::*;

#[derive(Debug, Clone)]
pub(crate) struct UpstreamCredentials {
    pub(crate) access_token: String,
    pub(crate) profile_arn: Option<String>,
    pub(crate) provider: Option<String>,
    pub(crate) region: String,
    /// 真实账号 ID（用于限流/失败/连接计数），不暴露给前端
    pub(crate) account_id: String,
    pub(crate) source_label: String,
    pub(crate) user_agent: String,
    #[allow(dead_code)]
    pub(crate) auth_method: Option<String>,
    pub(crate) send_opt_out: bool,
}


#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResponsesOutputText {
    pub(crate) text: String,
    pub(crate) annotations: Vec<Value>,
}

pub(crate) type UpstreamRequestError = (StatusCode, &'static str, String, Option<String>);

#[allow(dead_code)]
pub(crate) const STREAMING_RESPONSE_PLACEHOLDER: &str = "[streaming response omitted from request log]";


#[derive(Debug, Clone)]
pub(crate) struct RequestLogContext<'a> {
    pub(crate) request_index: u64,
    pub(crate) endpoint: &'a str,
    pub(crate) client_addr: SocketAddr,
    pub(crate) request: Option<&'a NormalizedRequest>,
    pub(crate) upstream: Option<&'a UpstreamCredentials>,
    pub(crate) started_at: Instant,
    #[allow(dead_code)]
    pub(crate) request_body: Option<&'a str>,
    /// 从原始请求体提取的 model（用于错误日志）
    pub(crate) model_hint: Option<String>,
    /// 是否流式请求（避免 request 为 None 时丢失信息）
    pub(crate) is_stream: Option<bool>,
}


#[derive(Debug, Clone, Copy)]
pub(crate) struct GatewayErrorDetails<'a> {
    pub(crate) status: StatusCode,
    pub(crate) error_type: &'static str,
    pub(crate) message: &'a str,
    pub(crate) response_body: Option<&'a str>,
}
