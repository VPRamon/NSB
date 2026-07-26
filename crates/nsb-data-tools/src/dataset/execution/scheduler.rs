//! Mockable scheduler contract and Slurm command adapter.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::ffi::{OsStr, OsString};
use std::process::Command;

/// Scheduler-independent array submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayRequest {
    pub job_name: String,
    pub indices: Vec<u32>,
    pub max_parallel: usize,
    pub wrapped_command: String,
    pub partition: Option<String>,
    pub account: Option<String>,
    pub time_limit: Option<String>,
    pub memory: Option<String>,
}

/// Accepted scheduler job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobSubmission {
    pub job_id: String,
}

/// Normalized scheduler state used by status and recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SchedulerState {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Unknown,
}

/// Scheduler operations needed by the dataset engine.
pub trait Scheduler {
    fn submit_array(&self, request: &ArrayRequest) -> Result<JobSubmission>;
    fn state(&self, job_id: &str) -> Result<SchedulerState>;
}

#[derive(Debug)]
pub struct ProcessResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Process boundary separated for deterministic scheduler tests.
pub trait ProcessRunner {
    fn run(&self, program: &OsStr, arguments: &[OsString]) -> Result<ProcessResult>;
}

/// Real operating-system process runner.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemProcessRunner;

impl ProcessRunner for SystemProcessRunner {
    fn run(&self, program: &OsStr, arguments: &[OsString]) -> Result<ProcessResult> {
        let output = Command::new(program)
            .args(arguments)
            .output()
            .with_context(|| format!("failed to execute {}", program.to_string_lossy()))?;
        Ok(ProcessResult {
            success: output.status.success(),
            stdout: String::from_utf8(output.stdout)?,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// Slurm implementation using `sbatch` and `sacct`.
#[derive(Debug, Clone)]
pub struct SlurmScheduler<R = SystemProcessRunner> {
    runner: R,
}

impl Default for SlurmScheduler<SystemProcessRunner> {
    fn default() -> Self {
        Self {
            runner: SystemProcessRunner,
        }
    }
}

impl<R> SlurmScheduler<R> {
    #[cfg(test)]
    pub fn with_runner(runner: R) -> Self {
        Self { runner }
    }
}

impl<R: ProcessRunner> Scheduler for SlurmScheduler<R> {
    fn submit_array(&self, request: &ArrayRequest) -> Result<JobSubmission> {
        if request.indices.is_empty() {
            bail!("scheduler array requires at least one task");
        }
        if request.max_parallel == 0 {
            bail!("scheduler array max_parallel must be greater than zero");
        }
        let array = format!(
            "{}%{}",
            compress_indices(&request.indices)?,
            request.max_parallel
        );
        let mut arguments = vec![
            "--parsable".into(),
            "--job-name".into(),
            request.job_name.clone().into(),
            "--array".into(),
            array.into(),
            "--wrap".into(),
            request.wrapped_command.clone().into(),
        ];
        push_option(&mut arguments, "--partition", request.partition.as_deref());
        push_option(&mut arguments, "--account", request.account.as_deref());
        push_option(&mut arguments, "--time", request.time_limit.as_deref());
        push_option(&mut arguments, "--mem", request.memory.as_deref());
        let result = self.runner.run(OsStr::new("sbatch"), &arguments)?;
        if !result.success {
            bail!("sbatch failed: {}", result.stderr.trim());
        }
        let job_id = result
            .stdout
            .trim()
            .split(';')
            .next()
            .unwrap_or_default()
            .to_string();
        if job_id.is_empty() || !job_id.bytes().all(|byte| byte.is_ascii_digit()) {
            bail!("sbatch returned invalid job id {:?}", result.stdout.trim());
        }
        Ok(JobSubmission { job_id })
    }

    fn state(&self, job_id: &str) -> Result<SchedulerState> {
        let arguments = [
            "--noheader".into(),
            "--jobs".into(),
            job_id.into(),
            "--format".into(),
            "State".into(),
            "--parsable2".into(),
        ];
        let result = self.runner.run(OsStr::new("sacct"), &arguments)?;
        if !result.success {
            bail!("sacct failed: {}", result.stderr.trim());
        }
        let states: Vec<SchedulerState> = result
            .stdout
            .lines()
            .filter_map(|line| line.split('|').next())
            .map(normalize_state)
            .collect();
        if states.is_empty() {
            return Ok(SchedulerState::Unknown);
        }
        Ok(aggregate_states(&states))
    }
}

fn push_option(arguments: &mut Vec<OsString>, flag: &str, value: Option<&str>) {
    if let Some(value) = value {
        arguments.push(flag.into());
        arguments.push(value.into());
    }
}

fn compress_indices(indices: &[u32]) -> Result<String> {
    let mut sorted = indices.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.len() != indices.len() {
        bail!("scheduler indices must be unique");
    }
    Ok(sorted
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(","))
}

fn normalize_state(raw: &str) -> SchedulerState {
    match raw.trim().trim_end_matches('+') {
        "PENDING" | "CONFIGURING" | "REQUEUED" | "RESIZING" => SchedulerState::Pending,
        "RUNNING" | "COMPLETING" | "SUSPENDED" => SchedulerState::Running,
        "COMPLETED" => SchedulerState::Succeeded,
        "CANCELLED" | "PREEMPTED" => SchedulerState::Cancelled,
        "FAILED" | "BOOT_FAIL" | "DEADLINE" | "NODE_FAIL" | "OUT_OF_MEMORY" | "TIMEOUT" => {
            SchedulerState::Failed
        }
        _ => SchedulerState::Unknown,
    }
}

fn aggregate_states(states: &[SchedulerState]) -> SchedulerState {
    for preferred in [
        SchedulerState::Failed,
        SchedulerState::Cancelled,
        SchedulerState::Running,
        SchedulerState::Pending,
        SchedulerState::Unknown,
        SchedulerState::Succeeded,
    ] {
        if states.contains(&preferred) {
            return preferred;
        }
    }
    SchedulerState::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeRunner {
        calls: Mutex<Vec<(OsString, Vec<OsString>)>>,
        results: Mutex<Vec<ProcessResult>>,
    }

    impl ProcessRunner for FakeRunner {
        fn run(&self, program: &OsStr, arguments: &[OsString]) -> Result<ProcessResult> {
            self.calls
                .lock()
                .unwrap()
                .push((program.to_os_string(), arguments.to_vec()));
            Ok(self.results.lock().unwrap().remove(0))
        }
    }

    #[test]
    fn submits_sparse_array_without_shell_partition_lookup() {
        let runner = FakeRunner {
            calls: Mutex::new(Vec::new()),
            results: Mutex::new(vec![ProcessResult {
                success: true,
                stdout: "4812;cluster\n".to_string(),
                stderr: String::new(),
            }]),
        };
        let scheduler = SlurmScheduler::with_runner(runner);
        let submission = scheduler
            .submit_array(&ArrayRequest {
                job_name: "nsb-starlight-build".to_string(),
                indices: vec![0, 3, 9],
                max_parallel: 2,
                wrapped_command: "exec '/opt/nsb-data' _worker --run '/data/run.json'".to_string(),
                partition: Some("compute".to_string()),
                account: None,
                time_limit: Some("12:00:00".to_string()),
                memory: Some("8G".to_string()),
            })
            .unwrap();
        assert_eq!(submission.job_id, "4812");
        let calls = scheduler.runner.calls.lock().unwrap();
        let arguments = &calls[0].1;
        assert!(arguments.contains(&OsString::from("0,3,9%2")));
        assert!(!arguments.iter().any(|argument| argument == "sed"));
    }

    #[test]
    fn aggregates_completed_array_with_one_failed_task_as_failed() {
        let runner = FakeRunner {
            calls: Mutex::new(Vec::new()),
            results: Mutex::new(vec![ProcessResult {
                success: true,
                stdout: "COMPLETED|\nCOMPLETED|\nOUT_OF_MEMORY|\n".to_string(),
                stderr: String::new(),
            }]),
        };
        let scheduler = SlurmScheduler::with_runner(runner);
        assert_eq!(scheduler.state("4812").unwrap(), SchedulerState::Failed);
    }
}
