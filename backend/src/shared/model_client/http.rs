//! HTTP 传输不记录 URL、请求头、正文或底层错误文本。
use super::{endpoint, ClientError, ExecutionOptions, Protocol, RequestTarget};
use futures_util::StreamExt;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client, Method, Request, Response, Url,
};
use std::time::Duration;
#[derive(Clone)]
pub(crate) struct Transport {
    client: Result<Client, ()>,
    defaults: HeaderMap,
}
impl Transport {
    pub(super) fn new() -> Self {
        let mut defaults = HeaderMap::new();
        defaults.insert("accept", HeaderValue::from_static("*/*"));
        defaults.insert(
            "user-agent",
            HeaderValue::from_static(super::user_agent::value()),
        );
        Self {
            client: Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(30))
                .default_headers(defaults.clone())
                .build()
                .map_err(|_| ()),
            defaults,
        }
    }
    /// 在发送前完成应用默认头、配置头及认证头的合并；配置重复头仍保持最后值覆盖。
    pub(super) fn request(
        &self,
        config: &RequestTarget,
        url: Url,
        body: Option<&serde_json::Value>,
        options: ExecutionOptions,
    ) -> Result<Request, ClientError> {
        let mut headers = HeaderMap::new();
        for h in &config.headers {
            let key =
                HeaderName::from_bytes(h.key.as_bytes()).map_err(|_| ClientError::Response)?;
            let value = HeaderValue::from_str(&h.value).map_err(|_| ClientError::Response)?;
            headers.insert(key, value);
        }
        endpoint::credential(&config.secret)?;
        if config.protocol == Protocol::Anthropic {
            let mut key =
                HeaderValue::from_str(&config.secret).map_err(|_| ClientError::Response)?;
            key.set_sensitive(true);
            headers.insert("x-api-key", key);
            if !headers.contains_key("anthropic-version") {
                headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
            }
        } else {
            let mut key = HeaderValue::from_str(&format!("Bearer {}", config.secret.as_str()))
                .map_err(|_| ClientError::Response)?;
            key.set_sensitive(true);
            headers.insert("authorization", key);
        }
        let mut request = self
            .client
            .as_ref()
            .map_err(|_| ClientError::Network)?
            .request(
                if body.is_some() {
                    Method::POST
                } else {
                    Method::GET
                },
                url,
            )
            .headers(headers)
            .timeout(options.total_timeout);
        if let Some(body) = body {
            request = request.json(body);
        }
        let mut request = request.build().map_err(ClientError::transport)?;
        // reqwest 原本在 execute 内注入这些默认头。提前合并使采集看到同一份最终头，
        // 不覆盖配置的值，也不改变认证头和 JSON Content-Type 的优先级。
        for (name, value) in &self.defaults {
            request.headers_mut().entry(name).or_insert(value.clone());
        }
        Ok(request)
    }
    pub(super) async fn execute(&self, request: Request) -> Result<Response, ClientError> {
        self.client
            .as_ref()
            .map_err(|_| ClientError::Network)?
            .execute(request)
            .await
            .map_err(ClientError::transport)
    }
    pub(crate) async fn send(
        &self,
        config: &RequestTarget,
        url: Url,
        body: Option<&serde_json::Value>,
        options: ExecutionOptions,
    ) -> Result<Response, ClientError> {
        self.execute(self.request(config, url, body, options)?)
            .await
    }
    /// 需要完整解析 JSON 的调用方才缓存正文，并显式指定内存缓冲上限。
    pub(crate) async fn read_buffered(
        &self,
        response: Response,
        limit: usize,
        options: ExecutionOptions,
    ) -> Result<Vec<u8>, ClientError> {
        if response.content_length().is_some_and(|n| n > limit as u64) {
            return Err(ClientError::Response);
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = tokio::time::timeout(options.idle_timeout, stream.next())
            .await
            .map_err(|_| ClientError::Timeout)?
        {
            let chunk = chunk.map_err(ClientError::transport)?;
            if bytes.len().saturating_add(chunk.len()) > limit {
                return Err(ClientError::Response);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}
