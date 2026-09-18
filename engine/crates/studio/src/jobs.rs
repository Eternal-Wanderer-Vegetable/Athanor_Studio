// Copyright (C) 2026 The Athanor Studio Developers
// SPDX-License-Identifier: AGPL-3.0-only

//! Conversion runs on disposable copies. Cancellation wins until the commit
//! boundary; once committing, callers must wait for the actual result.

use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::ipc::Channel;

const MAX_INPUT: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum JobRequest {
    Import {
        input: PathBuf,
        output: PathBuf,
        reader: Reader,
    },
    Export {
        input: PathBuf,
        output: PathBuf,
        format: ExportFormat,
    },
    Publish {
        input: PathBuf,
        output: PathBuf,
        no_paged: bool,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Reader {
    Auto,
    Native,
    Pandoc,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Markdown,
    Html,
    Text,
    Docx,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Queued,
    Running,
    Cancelling,
    Committing,
    Succeeded,
    Failed,
    Cancelled,
}

impl Phase {
    fn terminal(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct JobError {
    code: &'static str,
    message: String,
}

fn error(code: &'static str, message: impl ToString) -> JobError {
    JobError {
        code,
        message: message.to_string(),
    }
}

impl From<std::io::Error> for JobError {
    fn from(value: std::io::Error) -> Self {
        error("io_error", value)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct JobSnapshot {
    pub id: u64,
    pub phase: Phase,
    /// Milestones, not an estimate of conversion time.
    pub progress: u8,
    pub result: Option<Value>,
    pub error: Option<JobError>,
    /// 任务所属的文档会话（可空；供前端区分结果归属）。
    pub session_id: Option<String>,
}

struct Job {
    snapshot: Mutex<JobSnapshot>,
    updates: Option<Channel<JobSnapshot>>,
}

impl Job {
    fn notify(&self, snapshot: &JobSnapshot) {
        if let Some(channel) = &self.updates {
            let _ = channel.send(snapshot.clone());
        }
    }

    fn advance(&self, phase: Phase, progress: u8) -> Result<(), JobError> {
        let mut snapshot = self.snapshot.lock().unwrap();
        if snapshot.phase == Phase::Cancelling {
            return Err(error("cancelled", "任务已取消，未提交任何结果"));
        }
        snapshot.phase = phase;
        snapshot.progress = progress;
        self.notify(&snapshot);
        Ok(())
    }

    fn finish(&self, result: Result<Value, JobError>) {
        let mut snapshot = self.snapshot.lock().unwrap();
        match result {
            Ok(value) => {
                snapshot.phase = Phase::Succeeded;
                snapshot.result = Some(value);
            }
            Err(e) => {
                snapshot.phase = if e.code == "cancelled" {
                    Phase::Cancelled
                } else {
                    Phase::Failed
                };
                snapshot.error = Some(e);
            }
        }
        snapshot.progress = 100;
        self.notify(&snapshot);
    }
}

/// One conversion at a time bounds worker/browser resource usage. The latest
/// result remains queryable until another job starts; progress uses IPC Channel.
#[derive(Default)]
pub struct JobManager {
    current: Mutex<Option<Arc<Job>>>,
    pub(crate) commit_gate: Arc<Mutex<()>>,
}

impl JobManager {
    fn reserve(
        &self,
        updates: Option<Channel<JobSnapshot>>,
        session_id: Option<String>,
    ) -> Result<Arc<Job>, JobError> {
        let mut current = self.current.lock().unwrap();
        let id = if let Some(previous) = current.as_ref() {
            let snapshot = previous.snapshot.lock().unwrap();
            if !snapshot.phase.terminal() {
                return Err(error("job_busy", "已有任务正在运行，请等待或取消"));
            }
            snapshot.id + 1
        } else {
            1
        };
        let job = Arc::new(Job {
            snapshot: Mutex::new(JobSnapshot {
                id,
                phase: Phase::Queued,
                progress: 0,
                result: None,
                error: None,
                session_id,
            }),
            updates,
        });
        *current = Some(job.clone());
        Ok(job)
    }

    fn lookup(&self, id: u64) -> Result<Arc<Job>, JobError> {
        self.current
            .lock()
            .unwrap()
            .as_ref()
            .filter(|job| job.snapshot.lock().unwrap().id == id)
            .cloned()
            .ok_or_else(|| error("job_not_found", "任务不存在或结果已被后续任务替换"))
    }

    fn cancel(&self, id: u64) -> Result<bool, JobError> {
        let job = self.lookup(id)?;
        let mut snapshot = job.snapshot.lock().unwrap();
        match snapshot.phase {
            Phase::Queued | Phase::Running | Phase::Cancelling => {
                snapshot.phase = Phase::Cancelling;
                job.notify(&snapshot);
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

#[tauri::command]
pub fn run_job(
    state: tauri::State<'_, JobManager>,
    request: JobRequest,
    on_update: Channel<JobSnapshot>,
    session_id: Option<String>,
) -> Result<u64, JobError> {
    let job = state.reserve(Some(on_update), session_id)?;
    let id = job.snapshot.lock().unwrap().id;
    let gate = state.commit_gate.clone();
    let worker = job.clone();
    if let Err(e) = std::thread::Builder::new()
        .name(format!("studio-job-{id}"))
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                execute(&request, &worker, &gate)
            }))
            .unwrap_or_else(|_| Err(error("engine_panic", "转换引擎异常，任务未正常完成")));
            worker.finish(result);
        })
    {
        job.finish(Err(error("worker_unavailable", e)));
    }
    Ok(id)
}

#[tauri::command]
pub fn get_job(state: tauri::State<'_, JobManager>, job_id: u64) -> Result<JobSnapshot, JobError> {
    Ok(state.lookup(job_id)?.snapshot.lock().unwrap().clone())
}

#[tauri::command]
pub fn cancel_job(state: tauri::State<'_, JobManager>, job_id: u64) -> Result<bool, JobError> {
    state.cancel(job_id)
}

fn absolute_path(path: &Path) -> Result<(), JobError> {
    if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(error("invalid_path", "请输入不含 .. 的绝对路径"));
    }
    Ok(())
}

fn output_path(path: &Path) -> Result<PathBuf, JobError> {
    absolute_path(path)?;
    if fs::symlink_metadata(path).is_ok() {
        return Err(error("output_exists", "目标已存在，请选择新文件名"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| error("invalid_path", "缺少目标目录"))?
        .canonicalize()?;
    let name = path
        .file_name()
        .ok_or_else(|| error("invalid_path", "缺少目标文件名"))?;
    Ok(parent.join(name))
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, JobError> {
    let file = fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(error("invalid_path", "输入必须是普通文件"));
    }
    let mut data = Vec::new();
    file.take(MAX_INPUT + 1).read_to_end(&mut data)?;
    if data.len() as u64 > MAX_INPUT {
        return Err(error("input_too_large", "输入超过 64 MiB 限制"));
    }
    Ok(data)
}

fn latest_report(path: &Path) -> Result<Value, JobError> {
    let (mut container, _) =
        azodoc_container::open(fs::read(path)?).map_err(|e| error("invalid_container", e))?;
    let report_path = container.manifest_value()["reports"]
        .as_array()
        .and_then(|reports| reports.last())
        .and_then(|report| report["path"].as_str())
        .ok_or_else(|| error("missing_report", "转换结果缺少损失报告"))?
        .to_owned();
    let bytes = container
        .read_entry(&report_path)
        .map_err(|e| error("invalid_report", e))?;
    serde_json::from_slice(&bytes).map_err(|e| error("invalid_report", e))
}

fn staged_file(source: &Path, parent: &Path) -> Result<tempfile::NamedTempFile, JobError> {
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    std::io::copy(&mut fs::File::open(source)?, file.as_file_mut())?;
    file.as_file_mut().flush()?;
    file.as_file().sync_all()?;
    Ok(file)
}

fn replace_existing(destination: &Path, staged: &Path) -> Result<(), JobError> {
    let parent = destination
        .parent()
        .ok_or_else(|| error("invalid_path", "缺少目标目录"))?;
    let backup = tempfile::NamedTempFile::new_in(parent)?;
    let backup_path = backup.path().to_path_buf();
    drop(backup);
    fs::rename(destination, &backup_path)?;
    if let Err(commit_error) = fs::rename(staged, destination) {
        let _ = fs::rename(&backup_path, destination);
        return Err(error("output_commit_failed", commit_error));
    }
    let _ = fs::remove_file(backup_path);
    Ok(())
}

fn execute(request: &JobRequest, job: &Job, gate: &Mutex<()>) -> Result<Value, JobError> {
    job.advance(Phase::Running, 10)?;
    let (input, output) = match request {
        JobRequest::Import { input, output, .. }
        | JobRequest::Export { input, output, .. }
        | JobRequest::Publish { input, output, .. } => (input, output),
    };
    absolute_path(input)?;
    let input = input.canonicalize()?;
    let output = output_path(output)?;
    let original = {
        let _guard = gate.lock().unwrap();
        read_bounded(&input)?
    };
    let workspace = tempfile::tempdir_in(output.parent().unwrap())?;
    let container = workspace.path().join("document.azodoc");
    let artifact = workspace.path().join("artifact");
    let code = match request {
        JobRequest::Import { reader, .. } => {
            // Keep the original asset base directory (relative Markdown/HTML
            // images). The engine's import only reads this input.
            let reader = match reader {
                Reader::Auto => "auto",
                Reader::Native => "native",
                Reader::Pandoc => "pandoc",
            };
            athanor_cli::cmd_import_reader(&input, &container, None, "zh-CN", None, false, reader)
        }
        JobRequest::Export { format, .. } => {
            fs::write(&container, &original)?;
            let format = match format {
                ExportFormat::Markdown => "markdown",
                ExportFormat::Html => "html",
                ExportFormat::Text => "text",
                ExportFormat::Docx => "docx",
            };
            // Cache/report changes are confined to the working copy.
            athanor_cli::cmd_transmute(&container, format, Some(&artifact), false, false)
        }
        JobRequest::Publish { no_paged, .. } => {
            fs::write(&container, &original)?;
            athanor_cli::cmd_publish(
                &container,
                &athanor_cli::PublishArgs {
                    out: Some(&artifact),
                    browser: None,
                    no_paged: *no_paged,
                },
            )
        }
    };
    // This is the safe cancellation boundary after non-interruptible engine
    // calls (including Pandoc/Chromium). No user output has been touched yet.
    job.advance(Phase::Running, 80)?;
    if code != 0 {
        return Err(error(
            "conversion_failed",
            format!("转换失败（退出码 {code}）；请检查文件格式及 Pandoc/Chromium 是否可用"),
        ));
    }
    let report = latest_report(&container)?;
    let source = if matches!(request, JobRequest::Import { .. }) {
        &container
    } else {
        &artifact
    };
    let staged_output = staged_file(source, output.parent().unwrap())?;
    let publication = if matches!(request, JobRequest::Publish { .. }) {
        Some(staged_file(&container, input.parent().unwrap())?)
    } else {
        None
    };
    let _guard = gate.lock().unwrap();
    if publication.is_some() && read_bounded(&input)? != original {
        return Err(error(
            "document_conflict",
            "文档在出版期间发生变化，请重新出版",
        ));
    }
    job.advance(Phase::Committing, 90)?;
    staged_output
        .persist_noclobber(&output)
        .map_err(|e| error("output_commit_failed", e.error))?;
    if let Some(publication) = publication {
        if let Err(e) = replace_existing(&input, publication.path()) {
            // The PDF is a complete artifact. Report its location rather than
            // deleting a path which another process could already have replaced.
            return Err(error(
                "publication_commit_failed",
                format!(
                    "PDF 已写入 {}，出版记录回写失败：{}",
                    output.display(),
                    e.message
                ),
            ));
        }
    }
    Ok(
        json!({ "output": output, "document": if matches!(request, JobRequest::Import { .. }) { &output } else { &input }, "report": report }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn import_fixture(dir: &Path) -> (JobManager, JobRequest) {
        let input = dir.join("source.md");
        fs::write(&input, "# Studio\n\nHello **world**.\n").unwrap();
        (
            JobManager::default(),
            JobRequest::Import {
                input,
                output: dir.join("book.azodoc"),
                reader: Reader::Auto,
            },
        )
    }

    #[test]
    fn import_and_export_return_reports_without_changing_source() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, request) = import_fixture(dir.path());
        let job = manager.reserve(None, None).unwrap();
        let result = execute(&request, &job, &manager.commit_gate).unwrap();
        assert_eq!(result["report"]["direction"], "import");
        job.finish(Ok(result));
        let input = dir.path().join("book.azodoc");
        let before = fs::read(&input).unwrap();
        for format in [
            ExportFormat::Markdown,
            ExportFormat::Html,
            ExportFormat::Text,
            ExportFormat::Docx,
        ] {
            let output = dir.path().join(format!("output-{format:?}"));
            let job = manager.reserve(None, None).unwrap();
            let result = execute(
                &JobRequest::Export {
                    input: input.clone(),
                    output: output.clone(),
                    format,
                },
                &job,
                &manager.commit_gate,
            )
            .unwrap();
            assert_eq!(result["report"]["direction"], "export");
            assert!(!fs::read(output).unwrap().is_empty());
            assert_eq!(fs::read(&input).unwrap(), before);
            job.finish(Ok(result));
        }
    }

    #[test]
    fn cancellation_busy_and_commit_boundary_are_truthful() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, request) = import_fixture(dir.path());
        let job = manager.reserve(None, None).unwrap();
        assert_eq!(manager.reserve(None, None).err().unwrap().code, "job_busy");
        assert!(manager.cancel(1).unwrap());
        let result = execute(&request, &job, &manager.commit_gate);
        assert_eq!(result.as_ref().unwrap_err().code, "cancelled");
        job.finish(result);
        assert!(!dir.path().join("book.azodoc").exists());
        assert_eq!(job.snapshot.lock().unwrap().phase, Phase::Cancelled);
        assert!(!manager.cancel(1).unwrap());
        let job = manager.reserve(None, None).unwrap();
        job.advance(Phase::Running, 10).unwrap();
        assert!(manager.cancel(2).unwrap());
        assert_eq!(
            job.advance(Phase::Committing, 90).unwrap_err().code,
            "cancelled"
        );
        job.finish(Err(error("cancelled", "cancelled")));
        let job = manager.reserve(None, None).unwrap();
        job.advance(Phase::Committing, 90).unwrap();
        assert!(!manager.cancel(3).unwrap());
    }

    #[test]
    fn invalid_inputs_and_existing_outputs_never_write() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, request) = import_fixture(dir.path());
        let output = dir.path().join("book.azodoc");
        fs::write(&output, "existing").unwrap();
        let job = manager.reserve(None, None).unwrap();
        assert_eq!(
            execute(&request, &job, &manager.commit_gate)
                .unwrap_err()
                .code,
            "output_exists"
        );
        assert_eq!(fs::read(&output).unwrap(), b"existing");
        assert_eq!(
            absolute_path(Path::new("relative.md")).unwrap_err().code,
            "invalid_path"
        );
        assert_eq!(
            absolute_path(&dir.path().join("../escape"))
                .unwrap_err()
                .code,
            "invalid_path"
        );
        let huge = dir.path().join("huge");
        fs::File::create(&huge)
            .unwrap()
            .set_len(MAX_INPUT + 1)
            .unwrap();
        assert_eq!(read_bounded(&huge).unwrap_err().code, "input_too_large");
        let bad = dir.path().join("bad.azodoc");
        fs::write(&bad, "invalid").unwrap();
        let target = dir.path().join("failed.html");
        assert_eq!(
            execute(
                &JobRequest::Export {
                    input: bad,
                    output: target.clone(),
                    format: ExportFormat::Html
                },
                &job,
                &manager.commit_gate
            )
            .unwrap_err()
            .code,
            "conversion_failed"
        );
        assert!(!target.exists());

        let destination = dir.path().join("replace.azodoc");
        let staged = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
        fs::write(&destination, b"old").unwrap();
        fs::write(staged.path(), b"new").unwrap();
        replace_existing(&destination, staged.path()).unwrap();
        assert_eq!(fs::read(destination).unwrap(), b"new");
    }
}
