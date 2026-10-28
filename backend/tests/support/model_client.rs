//! 仅用于测试的真实本地 TCP 服务；记录请求并按测试脚本发送可控响应。
use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
#[derive(Clone)]
pub(crate) struct Response {
    pub status: u16,
    pub header_delay: Duration,
    pub content_type: &'static str,
    pub parts: Vec<(Duration, Vec<u8>)>,
    pub headers: Vec<(&'static str, String)>,
}
impl Response {
    pub(crate) fn json(value: serde_json::Value) -> Self {
        Self::raw(200, "application/json", value.to_string())
    }
    pub(crate) fn raw(status: u16, content_type: &'static str, body: impl Into<String>) -> Self {
        Self {
            status,
            header_delay: Duration::ZERO,
            content_type,
            parts: vec![(Duration::ZERO, body.into().into_bytes())],
            headers: vec![],
        }
    }
    pub(crate) fn delayed(mut self, delay: Duration) -> Self {
        self.parts[0].0 = delay;
        self
    }
    pub(crate) fn sse(events: &[(&str, u64)]) -> Self {
        Self {
            status: 200,
            header_delay: Duration::ZERO,
            content_type: "text/event-stream",
            parts: events
                .iter()
                .map(|(value, ms)| (Duration::from_millis(*ms), value.as_bytes().to_vec()))
                .collect(),
            headers: vec![],
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Request {
    pub head: String,
    pub body: Vec<u8>,
}
pub(crate) struct Server {
    pub url: String,
    requests: Arc<Mutex<Vec<Request>>>,
    stopping: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    pub(crate) fn new(responses: Vec<Response>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(vec![]));
        let stopping = Arc::new(AtomicBool::new(false));
        let captured = requests.clone();
        let stop = stopping.clone();
        let worker = thread::spawn(move || {
            let mut plans = VecDeque::from(responses);
            let mut workers = vec![];
            while !stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let plan = plans.pop_front().unwrap_or_else(|| {
                            Response::raw(500, "text/plain", "unexpected request")
                        });
                        let captured = captured.clone();
                        let stop = stop.clone();
                        workers.push(thread::spawn(move || serve(stream, plan, captured, stop)));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("test listener: {e}"),
                }
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            url,
            requests,
            stopping,
            worker: Some(worker),
        }
    }
    pub(crate) fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }
    pub(crate) async fn wait_requests(&self, count: usize) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while self.requests().len() < count {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
fn serve(
    mut stream: TcpStream,
    plan: Response,
    captured: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
) {
    // macOS 的 accept 会继承监听 socket 的非阻塞标志；工作线程使用阻塞读写。
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = vec![];
    let mut buffer = [0; 8192];
    let (head_end, body_len) = loop {
        let Ok(n) = stream.read(&mut buffer) else {
            return;
        };
        if n == 0 {
            return;
        }
        bytes.extend_from_slice(&buffer[..n]);
        if let Some(index) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8(bytes[..index].to_vec()).unwrap();
            let length = head
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            break (index + 4, length);
        }
    };
    while bytes.len() < head_end + body_len {
        let Ok(n) = stream.read(&mut buffer) else {
            return;
        };
        if n == 0 {
            return;
        }
        bytes.extend_from_slice(&buffer[..n]);
    }
    captured.lock().unwrap().push(Request {
        head: String::from_utf8(bytes[..head_end].to_vec()).unwrap(),
        body: bytes[head_end..head_end + body_len].to_vec(),
    });
    let mut head = format!(
        "HTTP/1.1 {} Test\r\nContent-Type: {}\r\nConnection: close\r\n",
        plan.status, plan.content_type
    );
    for (k, v) in plan.headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let until = std::time::Instant::now() + plan.header_delay;
    while std::time::Instant::now() < until {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        thread::sleep(Duration::from_millis(2));
    }
    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }
    for (delay, part) in plan.parts {
        let until = std::time::Instant::now() + delay;
        while std::time::Instant::now() < until {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            thread::sleep(Duration::from_millis(2));
        }
        if stream.write_all(&part).is_err() {
            return;
        }
    }
}

pub(crate) fn target(url: &str, protocol: super::Protocol) -> super::RequestTarget {
    super::RequestTarget {
        protocol,
        base_url: url.into(),
        headers: vec![],
        secret: zeroize::Zeroizing::new("local-test-secret".into()),
    }
}
pub(crate) fn request(url: &str, protocol: super::Protocol) -> super::CheckRequest {
    use super::*;
    let mut parameters = vec![Parameter::model()];
    if protocol != Protocol::CustomHttp {
        if protocol == Protocol::OpenaiResponses {
            parameters.push(Parameter::new("input", "只回复 OK", ParameterType::String));
        } else {
            parameters.push(Parameter::new(
                "messages",
                r#"[{"role":"user","content":"只回复 OK"}]"#,
                ParameterType::Json,
            ));
        }
        parameters.push(Parameter::new("stream", "true", ParameterType::Boolean));
        parameters.push(Parameter::new(
            if protocol == Protocol::OpenaiResponses {
                "max_output_tokens"
            } else {
                "max_tokens"
            },
            "8",
            ParameterType::Number,
        ));
    }
    CheckRequest {
        target: target(url, protocol),
        model: "remote-name".into(),
        endpoint_mode: EndpointMode::Relative,
        endpoint: match protocol {
            Protocol::OpenaiCompatible => "/v1/chat/completions",
            Protocol::OpenaiResponses => "/v1/responses",
            Protocol::Anthropic => "/v1/messages",
            Protocol::CustomHttp => "/probe",
        }
        .into(),
        body: if protocol == Protocol::CustomHttp {
            CheckBody::Custom { parameters }
        } else {
            CheckBody::Standard { parameters }
        },
    }
}

pub(crate) async fn check(
    client: &super::ModelClient,
    request: &super::CheckRequest,
) -> super::CheckResult {
    use super::{Cancellation, ExecutionOptions};
    client
        .check_once(
            request,
            ExecutionOptions::default(),
            &Cancellation::default(),
            None,
        )
        .await
        .unwrap()
}
