use std::{collections::HashSet, time::Duration};

use futures_util::{Stream, StreamExt};
use reqwest::{
    header::{
        HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, USER_AGENT,
    },
    Method, StatusCode, Url,
};
use serde_json::Value;
use thiserror::Error;

use crate::domain::{AuthPreset, HttpMethod, RequestHeader, SecretValue, Site, SiteProtocol};

const MAX_CATALOG_BYTES: u64 = 2 * 1024 * 1024;
const MAX_MODELS: usize = 10_000;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredModel {
    pub key: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Error)]
pub enum ModelCatalogError {
    #[error("the model catalog URL is invalid")]
    InvalidUrl,
    #[error("a model catalog request header is invalid")]
    InvalidHeader,
    #[error("the configured credential is missing")]
    MissingCredential,
    #[error("the model catalog request could not reach the server")]
    Network,
    #[error("the model catalog request timed out")]
    Timeout,
    #[error("the model catalog credential was rejected")]
    Authentication,
    #[error("the model catalog request was rate limited")]
    RateLimited,
    #[error("the model catalog server returned an error")]
    Upstream,
    #[error("the model catalog response is not supported")]
    Protocol,
}

pub type Result<T> = std::result::Result<T, ModelCatalogError>;

#[derive(Clone)]
pub struct ModelCatalogClient {
    client: reqwest::Client,
}

impl Default for ModelCatalogClient {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("static model catalog HTTP client configuration is valid"),
        }
    }
}

impl ModelCatalogClient {
    pub async fn discover(
        &self,
        site: &Site,
        headers: &[RequestHeader],
        secret: Option<&SecretValue>,
    ) -> Result<Vec<DiscoveredModel>> {
        let catalog_url = catalog_url(&site.base_url, &site.models_endpoint)?;
        let request_headers = request_headers(site, headers, secret)?;
        let response = self.send(site, catalog_url, request_headers).await?;
        parse_response(response).await
    }

    async fn send(&self, site: &Site, url: Url, headers: HeaderMap) -> Result<reqwest::Response> {
        let method = match site.models_method {
            HttpMethod::Get => Method::GET,
            HttpMethod::Post => Method::POST,
        };
        let mut request = self.client.request(method, url).headers(headers);
        if site.models_method == HttpMethod::Post {
            request = request.json(&serde_json::json!({}));
        }
        request.send().await.map_err(classify_transport_error)
    }
}

fn catalog_url(base_url: &str, endpoint: &str) -> Result<Url> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err(ModelCatalogError::InvalidUrl);
    }

    let mut base = Url::parse(base_url.trim()).map_err(|_| ModelCatalogError::InvalidUrl)?;
    validate_url(base.clone())?;

    if let Ok(url) = Url::parse(endpoint) {
        let url = validate_url(url)?;
        if url.scheme() != base.scheme()
            || url.host_str() != base.host_str()
            || url.port_or_known_default() != base.port_or_known_default()
        {
            return Err(ModelCatalogError::InvalidUrl);
        }
        return Ok(url);
    }

    if !base.path().ends_with('/') {
        let path = format!("{}/", base.path());
        base.set_path(&path);
    }
    base.join(endpoint.trim_start_matches('/'))
        .map_err(|_| ModelCatalogError::InvalidUrl)
        .and_then(validate_url)
}

fn validate_url(url: Url) -> Result<Url> {
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(ModelCatalogError::InvalidUrl);
    }
    Ok(url)
}

fn request_headers(
    site: &Site,
    configured: &[RequestHeader],
    secret: Option<&SecretValue>,
) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, header_value(&site.content_type)?);
    headers.insert(
        USER_AGENT,
        header_value(&super::outbound_user_agent("model-catalog"))?,
    );

    match site.auth_preset {
        AuthPreset::Bearer => {
            let secret = secret.ok_or(ModelCatalogError::MissingCredential)?;
            headers.insert(
                AUTHORIZATION,
                header_value(&format!("Bearer {}", secret.expose()))?,
            );
        }
        AuthPreset::XApiKey => {
            let secret = secret.ok_or(ModelCatalogError::MissingCredential)?;
            headers.insert("x-api-key", header_value(secret.expose())?);
        }
        AuthPreset::ApiKey => {
            let secret = secret.ok_or(ModelCatalogError::MissingCredential)?;
            headers.insert("api-key", header_value(secret.expose())?);
        }
        AuthPreset::None => {}
    }

    if site.protocol == SiteProtocol::Anthropic {
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    }

    for header in configured {
        let name = HeaderName::from_bytes(header.name.trim().as_bytes())
            .map_err(|_| ModelCatalogError::InvalidHeader)?;
        if name == CONTENT_LENGTH {
            return Err(ModelCatalogError::InvalidHeader);
        }
        headers.insert(name, header_value(&header.value_template)?);
    }
    Ok(headers)
}

fn header_value(value: &str) -> Result<HeaderValue> {
    HeaderValue::from_str(value).map_err(|_| ModelCatalogError::InvalidHeader)
}

fn classify_transport_error(error: reqwest::Error) -> ModelCatalogError {
    if error.is_timeout() {
        ModelCatalogError::Timeout
    } else {
        ModelCatalogError::Network
    }
}

async fn parse_response(response: reqwest::Response) -> Result<Vec<DiscoveredModel>> {
    match response.status() {
        status if status.is_success() => {}
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            return Err(ModelCatalogError::Authentication)
        }
        StatusCode::TOO_MANY_REQUESTS => return Err(ModelCatalogError::RateLimited),
        status if status.is_server_error() => return Err(ModelCatalogError::Upstream),
        _ => return Err(ModelCatalogError::Protocol),
    }

    if response
        .content_length()
        .is_some_and(|size| size > MAX_CATALOG_BYTES)
    {
        return Err(ModelCatalogError::Protocol);
    }
    let stream = response
        .bytes_stream()
        .map(|chunk| chunk.map_err(classify_transport_error));
    let body = read_limited_body(stream).await?;
    parse_catalog(&body)
}

async fn read_limited_body<S, B>(stream: S) -> Result<Vec<u8>>
where
    S: Stream<Item = Result<B>>,
    B: AsRef<[u8]>,
{
    futures_util::pin_mut!(stream);
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let chunk = chunk.as_ref();
        let next_len = body
            .len()
            .checked_add(chunk.len())
            .ok_or(ModelCatalogError::Protocol)?;
        let next_len = u64::try_from(next_len).map_err(|_| ModelCatalogError::Protocol)?;
        if next_len > MAX_CATALOG_BYTES {
            return Err(ModelCatalogError::Protocol);
        }
        body.extend_from_slice(chunk);
    }
    Ok(body)
}

fn parse_catalog(body: &[u8]) -> Result<Vec<DiscoveredModel>> {
    let value: Value = serde_json::from_slice(body).map_err(|_| ModelCatalogError::Protocol)?;
    let entries = match &value {
        Value::Array(entries) => entries,
        Value::Object(object) => object
            .get("data")
            .or_else(|| object.get("models"))
            .and_then(Value::as_array)
            .ok_or(ModelCatalogError::Protocol)?,
        _ => return Err(ModelCatalogError::Protocol),
    };
    if entries.len() > MAX_MODELS {
        return Err(ModelCatalogError::Protocol);
    }

    let mut seen = HashSet::new();
    let mut models = Vec::with_capacity(entries.len());
    for entry in entries {
        let model = parse_model(entry)?;
        if seen.insert(model.key.clone()) {
            models.push(model);
        }
    }
    Ok(models)
}

fn parse_model(value: &Value) -> Result<DiscoveredModel> {
    match value {
        Value::String(key) => Ok(DiscoveredModel {
            key: require_model_key(key)?,
            display_name: None,
        }),
        Value::Object(object) => {
            let id = object.get("id").and_then(Value::as_str);
            let name = object.get("name").and_then(Value::as_str);
            let key = require_model_key(id.or(name).ok_or(ModelCatalogError::Protocol)?)?;
            let display_name = object
                .get("display_name")
                .and_then(Value::as_str)
                .or(name.filter(|name| id.is_some() && *name != key))
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(ToOwned::to_owned);
            Ok(DiscoveredModel { key, display_name })
        }
        _ => Err(ModelCatalogError::Protocol),
    }
}

fn require_model_key(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ModelCatalogError::Protocol);
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        thread,
        time::Duration,
    };

    use crate::{domain::SecretValue, persistence::repository::test_support::site as fixture_site};

    use super::*;

    #[test]
    fn parses_openai_and_common_catalog_shapes_without_exact_duplicates() {
        let models = parse_catalog(
            br#"{"data":[{"id":"model-a","name":"Model A"},{"id":"MODEL-A"},{"id":"model-a"},{"id":"model-b"}]}"#,
        )
        .expect("catalog parses");
        assert_eq!(
            models,
            vec![
                DiscoveredModel {
                    key: "model-a".to_owned(),
                    display_name: Some("Model A".to_owned()),
                },
                DiscoveredModel {
                    key: "MODEL-A".to_owned(),
                    display_name: None,
                },
                DiscoveredModel {
                    key: "model-b".to_owned(),
                    display_name: None,
                },
            ]
        );

        assert_eq!(
            parse_catalog(br#"{"models":["model-c"]}"#).expect("models wrapper parses"),
            vec![DiscoveredModel {
                key: "model-c".to_owned(),
                display_name: None,
            }]
        );
    }

    #[test]
    fn rejects_malformed_or_blank_catalog_entries() {
        assert!(matches!(
            parse_catalog(br#"{"data":[{"object":"model"}]}"#),
            Err(ModelCatalogError::Protocol)
        ));
        assert!(matches!(
            parse_catalog(br#"["   "]"#),
            Err(ModelCatalogError::Protocol)
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn bounded_body_reader_stops_when_chunked_data_exceeds_the_limit() {
        let half_limit = usize::try_from(MAX_CATALOG_BYTES / 2).expect("limit fits");
        let chunks = futures_util::stream::iter([
            Ok::<_, ModelCatalogError>(vec![b'a'; half_limit]),
            Ok(vec![b'b'; half_limit]),
            Ok(vec![b'c']),
        ]);

        assert!(matches!(
            read_limited_body(chunks).await,
            Err(ModelCatalogError::Protocol)
        ));
    }

    #[test]
    fn joins_endpoints_without_dropping_a_base_path() {
        assert_eq!(
            catalog_url("https://example.test/v1", "/models")
                .expect("URL joins")
                .as_str(),
            "https://example.test/v1/models"
        );
        assert!(matches!(
            catalog_url("file:///tmp/catalog", "/models"),
            Err(ModelCatalogError::InvalidUrl)
        ));
    }

    #[test]
    fn absolute_catalog_endpoints_must_match_the_base_origin() {
        assert_eq!(
            catalog_url("https://example.test:443/v1", "https://EXAMPLE.test/models")
                .expect("default HTTPS ports share an origin")
                .as_str(),
            "https://example.test/models"
        );
        for endpoint in [
            "http://example.test/models",
            "https://catalog.example.test/models",
            "https://example.test:8443/models",
        ] {
            assert!(matches!(
                catalog_url("https://example.test/v1", endpoint),
                Err(ModelCatalogError::InvalidUrl)
            ));
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sends_bearer_auth_only_to_the_configured_catalog_path() {
        let (base_url, server) = spawn_server(vec![(
            200,
            r#"{"data":[{"id":"model-a"},{"id":"model-b"}]}"#,
        )]);
        let mut site = fixture_site("site-a", 0);
        site.base_url = base_url;
        site.protocol = SiteProtocol::OpenAiResponses;
        site.models_endpoint = "/configured/models".to_owned();
        let secret = SecretValue::new("fixture-secret").expect("secret is valid");

        let models = ModelCatalogClient::default()
            .discover(&site, &[], Some(&secret))
            .await
            .expect("catalog is discovered");
        let requests = server.join().expect("server joins");

        assert_eq!(
            models
                .iter()
                .map(|model| model.key.as_str())
                .collect::<Vec<_>>(),
            vec!["model-a", "model-b"]
        );
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET /configured/models HTTP/1.1"));
        assert!(requests.iter().all(|request| request
            .to_ascii_lowercase()
            .contains("authorization: bearer fixture-secret")));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn classifies_catalog_authentication_failures() {
        let (base_url, server) = spawn_server(vec![(401, r#"{"error":"unauthorized"}"#)]);
        let mut site = fixture_site("site-a", 0);
        site.base_url = base_url;
        let secret = SecretValue::new("fixture-secret").expect("secret is valid");

        let result = ModelCatalogClient::default()
            .discover(&site, &[], Some(&secret))
            .await;
        server.join().expect("server joins");

        assert!(matches!(result, Err(ModelCatalogError::Authentication)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rejects_an_oversized_body_without_a_content_length() {
        let (base_url, server) = spawn_raw_server(|stream| {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
                )
                .expect("response headers written");
            let body = vec![b' '; usize::try_from(MAX_CATALOG_BYTES).expect("limit fits") + 1];
            let _ = stream.write_all(&body);
        });
        let mut site = fixture_site("site-a", 0);
        site.base_url = base_url;
        let secret = SecretValue::new("fixture-secret").expect("secret is valid");

        let result = ModelCatalogClient::default()
            .discover(&site, &[], Some(&secret))
            .await;
        server.join().expect("server joins");

        assert!(matches!(result, Err(ModelCatalogError::Protocol)));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn classifies_a_stalled_response_as_a_timeout() {
        let (base_url, server) = spawn_raw_server(|stream| {
            thread::sleep(Duration::from_millis(200));
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"data\":[]}",
            );
        });
        let mut site = fixture_site("site-a", 0);
        site.base_url = base_url;
        let secret = SecretValue::new("fixture-secret").expect("secret is valid");
        let client = ModelCatalogClient {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(1))
                .timeout(Duration::from_millis(50))
                .build()
                .expect("test client builds"),
        };

        let result = client.discover(&site, &[], Some(&secret)).await;
        server.join().expect("server joins");

        assert!(matches!(result, Err(ModelCatalogError::Timeout)));
    }

    fn spawn_server(
        responses: Vec<(u16, &'static str)>,
    ) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test server binds");
        let address = listener.local_addr().expect("test server has address");
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().expect("test request accepted");
                requests.push(read_request(&mut stream));
                let reason = match status {
                    200 => "OK",
                    401 => "Unauthorized",
                    404 => "Not Found",
                    _ => "Test",
                };
                let response = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("test response written");
            }
            requests
        });
        (format!("http://{address}"), server)
    }

    fn spawn_raw_server(
        write_response: impl FnOnce(&mut TcpStream) + Send + 'static,
    ) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test server binds");
        let address = listener.local_addr().expect("test server has address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("test request accepted");
            let request = read_request(&mut stream);
            write_response(&mut stream);
            request
        });
        (format!("http://{address}"), server)
    }

    fn read_request(stream: &mut TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("read timeout configured");
        let mut buffer = [0_u8; 8 * 1024];
        let mut request = Vec::new();
        loop {
            let read = stream.read(&mut buffer).expect("test request read");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        String::from_utf8(request).expect("request is UTF-8")
    }
}
