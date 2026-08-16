use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use serde::{Deserialize, Serialize};
use tauri::{webview::NewWindowResponse, WebviewUrl};
use uuid::Uuid;

use super::{
    service::{IntelligenceScrapeError, IntelligenceScraper},
    RawRankingEntry,
};

const CHILD_FLAG: &str = "--intelligence-ladder-scraper";
const CHILD_HARD_TIMEOUT: Duration = Duration::from_secs(35);
const PARENT_HARD_TIMEOUT: Duration = Duration::from_secs(40);
const TERMINATION_GRACE: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(3);
const REQUIRED_STABLE_POLLS: usize = 3;
const MAX_PROTOCOL_BYTES: u64 = 64 * 1024;
const SOURCE_URL: &str = "https://codexradar.com/";

const EXTRACT_SCRIPT: &str = r#"
(() => {
  const entries = Array.from(document.querySelectorAll('[data-model][data-effort]'))
    .slice(0, 101)
    .map((node) => {
      const model = (node.getAttribute('data-model') || '').slice(0, 256);
      const effort = (node.getAttribute('data-effort') || '').slice(0, 80);
      const iqText = node.querySelector('.intelligence-efficiency-card-iq strong')?.textContent || '';
      const match = iqText.match(/-?\d+(?:\.\d+)?/);
      return match ? { model, effort, iq: Number(match[0]) } : null;
    })
    .filter(Boolean);
  return { entries };
})()
"#;

#[derive(Debug, Default)]
pub struct ProcessIntelligenceScraper;

impl IntelligenceScraper for ProcessIntelligenceScraper {
    fn scrape(
        &self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<RawRankingEntry>, IntelligenceScrapeError>>
                + Send,
        >,
    > {
        Box::pin(async {
            tokio::task::spawn_blocking(supervise_scraper)
                .await
                .map_err(|_| IntelligenceScrapeError::Internal)?
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkerOutput {
    job_id: String,
    #[serde(default)]
    entries: Vec<RawRankingEntry>,
    #[serde(default)]
    error: Option<WorkerError>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerError {
    code: WorkerErrorCode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WorkerErrorCode {
    RenderTimeout,
    RenderUnavailable,
    TargetNotFound,
    InvalidSnapshot,
    Internal,
}

pub fn scraper_sidecar_args() -> Option<(String, u32)> {
    let mut arguments = std::env::args();
    let _program = arguments.next()?;
    if arguments.next()?.as_str() != CHILD_FLAG {
        return None;
    }
    let job_id = arguments.next()?;
    let parent_pid = arguments.next()?.parse().ok()?;
    if arguments.next().is_some() || Uuid::parse_str(&job_id).is_err() || parent_pid == 0 {
        return None;
    }
    Some((job_id, parent_pid))
}

fn supervise_scraper() -> Result<Vec<RawRankingEntry>, IntelligenceScrapeError> {
    let executable =
        std::env::current_exe().map_err(|_| IntelligenceScrapeError::RenderUnavailable)?;
    let job_id = Uuid::new_v4().to_string();
    let mut command = Command::new(executable);
    command
        .arg(CHILD_FLAG)
        .arg(&job_id)
        .arg(std::process::id().to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|_| IntelligenceScrapeError::RenderUnavailable)?;
    let stdout = child
        .stdout
        .take()
        .ok_or(IntelligenceScrapeError::WorkerProtocolInvalid)?;
    let (output_sender, output_receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = output_sender.send(read_bounded_protocol_line(stdout));
    });
    let started_at = Instant::now();
    loop {
        match output_receiver.try_recv() {
            Ok(output) => {
                let result = output.and_then(|output| validate_worker_output(&job_id, &output));
                finish_worker_after_protocol(&mut child)?;
                return result;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                return match finish_worker_after_protocol(&mut child) {
                    Ok(()) => Err(IntelligenceScrapeError::WorkerProtocolInvalid),
                    Err(error) => Err(error),
                };
            }
        }
        match child.try_wait() {
            Ok(Some(_)) => {
                let pid = child.id();
                if !process_tree_is_gone(pid) {
                    terminate_process_tree(pid);
                }
                return if wait_for_process_tree_exit(pid, TERMINATION_GRACE) {
                    Err(IntelligenceScrapeError::WorkerCrashed)
                } else {
                    Err(IntelligenceScrapeError::WorkerKillFailed)
                };
            }
            Ok(None) if started_at.elapsed() < PARENT_HARD_TIMEOUT => {
                thread::sleep(POLL_INTERVAL);
            }
            Ok(None) => return terminate_worker(&mut child),
            Err(_) => {
                let pid = child.id();
                terminate_process_tree(pid);
                let _ = child.kill();
                let _ = child.wait();
                return if wait_for_process_tree_exit(pid, TERMINATION_GRACE) {
                    Err(IntelligenceScrapeError::WorkerCrashed)
                } else {
                    Err(IntelligenceScrapeError::WorkerKillFailed)
                };
            }
        }
    }
}

fn read_bounded_protocol_line(
    stdout: std::process::ChildStdout,
) -> Result<Vec<u8>, IntelligenceScrapeError> {
    let mut reader = BufReader::new(stdout);
    let mut output = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .map_err(|_| IntelligenceScrapeError::WorkerProtocolInvalid)?;
        if available.is_empty() {
            return Err(IntelligenceScrapeError::WorkerProtocolInvalid);
        }
        let consumed = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |index| index + 1);
        if output.len() + consumed > MAX_PROTOCOL_BYTES as usize {
            return Err(IntelligenceScrapeError::WorkerProtocolInvalid);
        }
        let complete = available[consumed - 1] == b'\n';
        output.extend_from_slice(&available[..consumed]);
        reader.consume(consumed);
        if complete {
            output.pop();
            return Ok(output);
        }
    }
}

fn finish_worker_after_protocol(
    child: &mut std::process::Child,
) -> Result<(), IntelligenceScrapeError> {
    let pid = child.id();
    #[cfg(windows)]
    {
        if !terminate_process_tree(pid) {
            return Err(IntelligenceScrapeError::WorkerKillFailed);
        }
        child
            .wait()
            .map_err(|_| IntelligenceScrapeError::WorkerKillFailed)?;
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        child.stdin.take();
        let exited = wait_for_child(child, TERMINATION_GRACE)?;
        if !exited || !process_tree_is_gone(pid) {
            terminate_process_tree(pid);
            let _ = child.kill();
            let _ = child.wait();
        }
        if wait_for_process_tree_exit(pid, TERMINATION_GRACE) {
            Ok(())
        } else {
            Err(IntelligenceScrapeError::WorkerKillFailed)
        }
    }
}

fn validate_worker_output(
    expected_job_id: &str,
    output: &[u8],
) -> Result<Vec<RawRankingEntry>, IntelligenceScrapeError> {
    let output: WorkerOutput = serde_json::from_slice(output)
        .map_err(|_| IntelligenceScrapeError::WorkerProtocolInvalid)?;
    if output.job_id != expected_job_id || (output.error.is_some() != output.entries.is_empty()) {
        return Err(IntelligenceScrapeError::WorkerProtocolInvalid);
    }
    if let Some(error) = output.error {
        return Err(match error.code {
            WorkerErrorCode::RenderTimeout => IntelligenceScrapeError::RenderTimeout,
            WorkerErrorCode::RenderUnavailable => IntelligenceScrapeError::RenderUnavailable,
            WorkerErrorCode::TargetNotFound => IntelligenceScrapeError::TargetNotFound,
            WorkerErrorCode::InvalidSnapshot => IntelligenceScrapeError::InvalidSnapshot,
            WorkerErrorCode::Internal => IntelligenceScrapeError::Internal,
        });
    }
    Ok(output.entries)
}

fn terminate_worker(
    child: &mut std::process::Child,
) -> Result<Vec<RawRankingEntry>, IntelligenceScrapeError> {
    let pid = child.id();
    child.stdin.take();
    let exited = wait_for_child(child, TERMINATION_GRACE)?;
    if !exited || !process_tree_is_gone(pid) {
        terminate_process_tree(pid);
        let _ = child.kill();
        let _ = child.wait();
    }
    if wait_for_process_tree_exit(pid, TERMINATION_GRACE) {
        Err(IntelligenceScrapeError::WorkerTimeout)
    } else {
        Err(IntelligenceScrapeError::WorkerKillFailed)
    }
}

fn wait_for_child(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Result<bool, IntelligenceScrapeError> {
    let started_at = Instant::now();
    while started_at.elapsed() < timeout {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(true),
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(_) => return Err(IntelligenceScrapeError::WorkerKillFailed),
        }
    }
    Ok(false)
}

fn wait_for_process_tree_exit(pid: u32, timeout: Duration) -> bool {
    let started_at = Instant::now();
    while started_at.elapsed() < timeout {
        if process_tree_is_gone(pid) {
            return true;
        }
        thread::sleep(POLL_INTERVAL);
    }
    process_tree_is_gone(pid)
}

#[cfg(unix)]
fn terminate_process_tree(pid: u32) -> bool {
    Command::new("/bin/kill")
        .arg("-KILL")
        .arg(format!("-{pid}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(windows)]
fn terminate_process_tree(pid: u32) -> bool {
    Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(not(any(unix, windows)))]
fn terminate_process_tree(_pid: u32) -> bool {
    false
}

#[cfg(unix)]
fn process_tree_is_gone(pid: u32) -> bool {
    !Command::new("/bin/kill")
        .arg("-0")
        .arg(format!("-{pid}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(not(unix))]
fn process_tree_is_gone(_pid: u32) -> bool {
    true
}

pub fn run_scraper_sidecar(job_id: String, _parent_pid: u32) -> i32 {
    let mut context = crate::app_context();
    context.config_mut().app.windows.clear();
    let build = tauri::Builder::default()
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let url = SOURCE_URL.parse().expect("fixed source URL is valid");
            let window = tauri::WebviewWindowBuilder::new(
                app,
                "intelligence-ladder-scraper",
                WebviewUrl::External(url),
            )
            .visible(false)
            .decorations(false)
            .skip_taskbar(true)
            .incognito(true)
            .devtools(false)
            .on_navigation(|url| {
                url.scheme() == "https"
                    && url.host_str() == Some("codexradar.com")
                    && url.port_or_known_default() == Some(443)
            })
            .on_new_window(|_, _| NewWindowResponse::Deny)
            .build()?;

            start_parent_pipe_lease();

            start_parent_lease(_parent_pid);

            let worker_app = app.handle().clone();
            thread::spawn(move || poll_rendered_page(worker_app, window, job_id));
            Ok(())
        })
        .build(context);
    let app = match build {
        Ok(app) => app,
        Err(_) => return 2,
    };
    app.run_return(|_, _| {})
}

fn poll_rendered_page(app: tauri::AppHandle, window: tauri::WebviewWindow, job_id: String) {
    let started_at = Instant::now();
    let mut saw_callback = false;
    let mut saw_invalid_candidate = false;
    let mut stable_candidate: Option<Vec<RawRankingEntry>> = None;
    let mut stable_polls = 0;
    thread::sleep(Duration::from_secs(1));
    while started_at.elapsed() < CHILD_HARD_TIMEOUT {
        let (sender, receiver) = mpsc::sync_channel(1);
        if window
            .eval_with_callback(EXTRACT_SCRIPT, move |value| {
                let _ = sender.send(value);
            })
            .is_err()
        {
            finish_worker(
                &app,
                &window,
                WorkerOutput::failure(job_id, WorkerErrorCode::RenderUnavailable),
            );
            return;
        }
        if let Ok(value) = receiver.recv_timeout(CALLBACK_TIMEOUT) {
            saw_callback = true;
            if let Some(candidate) = parse_rendered_candidate(&value) {
                let entry_count = candidate.entries.len();
                if (3..=100).contains(&entry_count) {
                    if super::parse_rankings(candidate.entries.clone()).is_ok() {
                        if stable_candidate.as_ref() == Some(&candidate.entries) {
                            stable_polls += 1;
                        } else {
                            stable_candidate = Some(candidate.entries);
                            stable_polls = 1;
                        }
                        if stable_polls >= REQUIRED_STABLE_POLLS {
                            finish_worker(
                                &app,
                                &window,
                                WorkerOutput::success(
                                    job_id,
                                    stable_candidate.take().expect("stable candidate exists"),
                                ),
                            );
                            return;
                        }
                    } else {
                        saw_invalid_candidate = true;
                        stable_candidate = None;
                        stable_polls = 0;
                    }
                } else {
                    stable_candidate = None;
                    stable_polls = 0;
                }
                if entry_count > 100 {
                    finish_worker(
                        &app,
                        &window,
                        WorkerOutput::failure(job_id, WorkerErrorCode::InvalidSnapshot),
                    );
                    return;
                }
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
    let code = if saw_invalid_candidate {
        WorkerErrorCode::InvalidSnapshot
    } else if saw_callback {
        WorkerErrorCode::TargetNotFound
    } else {
        WorkerErrorCode::RenderTimeout
    };
    finish_worker(&app, &window, WorkerOutput::failure(job_id, code));
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RenderedCandidate {
    entries: Vec<RawRankingEntry>,
}

fn parse_rendered_candidate(value: &str) -> Option<RenderedCandidate> {
    serde_json::from_str(value).ok().or_else(|| {
        let nested = serde_json::from_str::<String>(value).ok()?;
        serde_json::from_str(&nested).ok()
    })
}

impl WorkerOutput {
    fn success(job_id: String, entries: Vec<RawRankingEntry>) -> Self {
        Self {
            job_id,
            entries,
            error: None,
        }
    }

    fn failure(job_id: String, code: WorkerErrorCode) -> Self {
        Self {
            job_id,
            entries: Vec::new(),
            error: Some(WorkerError { code }),
        }
    }
}

fn finish_worker(_app: &tauri::AppHandle, _window: &tauri::WebviewWindow, output: WorkerOutput) {
    if let Ok(value) = serde_json::to_vec(&output) {
        if value.len() as u64 <= MAX_PROTOCOL_BYTES {
            let mut stdout = std::io::stdout();
            let _ = stdout.write_all(&value);
            let _ = stdout.write_all(b"\n");
            let _ = stdout.flush();
        }
    }
}

fn start_parent_lease(parent_pid: u32) {
    let own_pid = std::process::id();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(500));
        if !process_exists(parent_pid) {
            hard_terminate_own_process_tree(own_pid, 4);
        }
    });
}

fn start_parent_pipe_lease() {
    thread::spawn(|| {
        wait_for_parent_pipe_disconnect(std::io::stdin());
        hard_terminate_own_process_tree(std::process::id(), 3);
    });
}

fn wait_for_parent_pipe_disconnect(mut input: impl Read) {
    let mut buffer = [0_u8; 64];
    loop {
        match input.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
    }
}

fn hard_terminate_own_process_tree(own_pid: u32, exit_code: i32) -> ! {
    let _ = terminate_process_tree(own_pid);
    std::process::exit(exit_code);
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    Command::new("/bin/kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(windows)]
fn process_exists(pid: u32) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
}

#[cfg(not(any(unix, windows)))]
fn process_exists(_pid: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_rejects_wrong_job_extra_fields_and_mixed_results() {
        let job = Uuid::new_v4().to_string();
        let valid = serde_json::to_vec(&WorkerOutput::success(
            job.clone(),
            vec![RawRankingEntry {
                model: "A".to_owned(),
                effort: "high".to_owned(),
                iq: 100.0,
            }],
        ))
        .expect("protocol serializes");
        assert_eq!(
            validate_worker_output(&job, &valid)
                .expect("protocol is valid")
                .len(),
            1
        );
        assert!(matches!(
            validate_worker_output("wrong-job", &valid),
            Err(IntelligenceScrapeError::WorkerProtocolInvalid)
        ));
        assert!(matches!(
            validate_worker_output(&job, br#"{"jobId":"x","entries":[],"unknown":true}"#),
            Err(IntelligenceScrapeError::WorkerProtocolInvalid)
        ));

        let mixed = serde_json::to_vec(&WorkerOutput {
            job_id: job.clone(),
            entries: vec![RawRankingEntry {
                model: "A".to_owned(),
                effort: "high".to_owned(),
                iq: 100.0,
            }],
            error: Some(WorkerError {
                code: WorkerErrorCode::Internal,
            }),
        })
        .expect("protocol serializes");
        assert!(matches!(
            validate_worker_output(&job, &mixed),
            Err(IntelligenceScrapeError::WorkerProtocolInvalid)
        ));
    }

    #[test]
    fn rendered_callback_accepts_direct_and_nested_json_serialization() {
        let direct = r#"{"entries":[{"model":"A","effort":"high","iq":100.0}]}"#;
        let nested = serde_json::to_string(direct).expect("nested callback serializes");

        assert_eq!(
            parse_rendered_candidate(direct)
                .expect("direct callback parses")
                .entries
                .len(),
            1
        );
        assert_eq!(
            parse_rendered_candidate(&nested)
                .expect("nested callback parses")
                .entries
                .len(),
            1
        );
    }

    #[test]
    fn parent_pipe_lease_drains_bounded_chunks_until_eof() {
        wait_for_parent_pipe_disconnect(&b"parent-liveness-token"[..]);
    }
}
