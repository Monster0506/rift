use crate::job_manager::{CancellationSignal, Job, JobMessage, JobPayload, JobRef, JobTarget};
use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const READ_CHUNK: usize = 64 * 1024;
const EXIT_POLL: Duration = Duration::from_millis(10);
const KILLED_EXIT_CODE: i32 = 137;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JobCommand {
    Program { program: String, args: Vec<String> },
    Shell(String),
}

impl JobCommand {
    fn command(&self) -> Command {
        match self {
            Self::Program { program, args } => {
                let mut command = Command::new(program);
                command.args(args);
                command
            }
            Self::Shell(line) => shell_command(line),
        }
    }
}

impl std::fmt::Display for JobCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Program { program, .. } => f.write_str(program),
            Self::Shell(line) => f.write_str(line),
        }
    }
}

#[cfg(windows)]
fn shell_command(line: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new("cmd");
    command.raw_arg(format!("/S /C \"{line}\""));
    command
}

#[cfg(not(windows))]
fn shell_command(line: &str) -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", line]);
    command
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JobSpec {
    pub(crate) command: JobCommand,
    pub(crate) cwd: Option<String>,
    pub(crate) env: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PluginJobEvent {
    Output(JobStream, Vec<String>),
    Exit(i32),
    Failed(String),
}

impl PluginJobEvent {
    pub(crate) const fn is_final(&self) -> bool {
        matches!(self, Self::Exit(_) | Self::Failed(_))
    }
}

#[derive(Debug)]
pub struct PluginJobPayload {
    pub(crate) job: u64,
    pub(crate) event: PluginJobEvent,
}

impl JobPayload for PluginJobPayload {
    fn apply(self: Box<Self>, editor: &mut dyn JobTarget, _job: JobRef) {
        editor.apply_plugin_job(*self);
    }
}

#[derive(Debug)]
enum JobControl {
    Stdin(Vec<u8>),
    CloseStdin,
    StreamClosed,
}

#[derive(Debug)]
struct PluginProcess {
    child: Mutex<Option<Child>>,
    #[cfg(windows)]
    job_object: Option<std::os::windows::io::OwnedHandle>,
}

impl PluginProcess {
    fn kill(&self) {
        let mut slot = self.child.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(child) = slot.as_mut() else {
            return;
        };
        #[cfg(windows)]
        let killed = self
            .job_object
            .as_ref()
            .is_some_and(|job| windows_job::terminate(job, KILLED_EXIT_CODE.unsigned_abs()));
        #[cfg(unix)]
        let killed = kill_process_group(child);
        #[cfg(not(any(unix, windows)))]
        let killed = false;
        if !killed {
            let _ = child.kill();
        }
    }

    fn try_wait(&self) -> Option<i32> {
        let mut slot = self.child.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(child) = slot.as_mut() else {
            return Some(-1);
        };
        let code = match child.try_wait() {
            Ok(None) => return None,
            Ok(Some(status)) => exit_code(status),
            Err(_) => -1,
        };
        *slot = None;
        Some(code)
    }
}

#[cfg(unix)]
fn kill_process_group(child: &Child) -> bool {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    const SIGKILL: i32 = 9;
    let Ok(group) = i32::try_from(child.id()) else {
        return false;
    };
    unsafe { kill(-group, SIGKILL) == 0 }
}

fn exit_code(status: ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    status.code().unwrap_or(KILLED_EXIT_CODE)
}

#[cfg(windows)]
mod windows_job {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attributes: *const std::ffi::c_void, name: *const u16) -> RawHandle;
        fn AssignProcessToJobObject(job: RawHandle, process: RawHandle) -> i32;
        fn TerminateJobObject(job: RawHandle, exit_code: u32) -> i32;
    }

    pub(super) fn contain(child: &std::process::Child) -> Option<OwnedHandle> {
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if raw.is_null() {
            return None;
        }
        let job = unsafe { OwnedHandle::from_raw_handle(raw) };
        let assigned =
            unsafe { AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) };
        (assigned != 0).then_some(job)
    }

    pub(super) fn terminate(job: &OwnedHandle, exit_code: u32) -> bool {
        unsafe { TerminateJobObject(job.as_raw_handle(), exit_code) != 0 }
    }
}

#[derive(Debug)]
pub(crate) struct PluginJobHandle {
    process: Arc<PluginProcess>,
    control: Sender<JobControl>,
}

impl PluginJobHandle {
    pub(crate) fn send(&self, data: Vec<u8>) {
        let _ = self.control.send(JobControl::Stdin(data));
    }

    pub(crate) fn close_stdin(&self) {
        let _ = self.control.send(JobControl::CloseStdin);
    }

    pub(crate) fn stop(&self) {
        self.process.kill();
    }

    #[cfg(test)]
    pub(crate) fn exit_probe(&self) -> ExitProbe {
        ExitProbe(Arc::clone(&self.process))
    }
}

#[cfg(test)]
pub(crate) struct ExitProbe(Arc<PluginProcess>);

#[cfg(test)]
impl ExitProbe {
    pub(crate) fn has_exited(&self) -> bool {
        let mut slot = self.0.child.lock().unwrap_or_else(PoisonError::into_inner);
        slot.as_mut()
            .is_none_or(|child| matches!(child.try_wait(), Ok(Some(_))))
    }
}

impl Drop for PluginJobHandle {
    fn drop(&mut self) {
        self.process.kill();
    }
}

#[derive(Debug)]
pub(crate) struct PluginProcessJob {
    job: u64,
    process: Arc<PluginProcess>,
    stdin: ChildStdin,
    stdout: std::process::ChildStdout,
    stderr: std::process::ChildStderr,
    control: Receiver<JobControl>,
    control_tx: Sender<JobControl>,
}

pub(crate) fn start(
    job: u64,
    spec: &JobSpec,
) -> std::io::Result<(PluginJobHandle, PluginProcessJob)> {
    let mut command = spec.command.command();
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .envs(spec.env.iter().map(|(key, value)| (key, value)));
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    #[cfg(windows)]
    let job_object = windows_job::contain(&child);
    let (Some(stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(std::io::Error::other("child stdio was not piped"));
    };
    let process = Arc::new(PluginProcess {
        child: Mutex::new(Some(child)),
        #[cfg(windows)]
        job_object,
    });
    let (control_tx, control) = mpsc::channel();
    let handle = PluginJobHandle {
        process: Arc::clone(&process),
        control: control_tx.clone(),
    };
    let job = PluginProcessJob {
        job,
        process,
        stdin,
        stdout,
        stderr,
        control,
        control_tx,
    };
    Ok((handle, job))
}

impl Job for PluginProcessJob {
    fn run(self: Box<Self>, id: usize, sender: Sender<JobMessage>, _signal: CancellationSignal) {
        let Self {
            job,
            process,
            stdin,
            stdout,
            stderr,
            control,
            control_tx,
        } = *self;
        let readers = [
            spawn_reader(stdout, JobStream::Stdout, job, id, &sender, &control_tx),
            spawn_reader(stderr, JobStream::Stderr, job, id, &sender, &control_tx),
        ];
        drop(control_tx);
        let code = serve(&process, stdin, &control, readers.len());
        for reader in readers {
            let _ = reader.join();
        }
        let _ = sender.send(JobMessage::Custom(
            id,
            Box::new(PluginJobPayload {
                job,
                event: PluginJobEvent::Exit(code),
            }),
        ));
        let _ = sender.send(JobMessage::Finished(id, true));
    }

    fn is_silent(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "plugin job"
    }

    fn is_long_lived(&self) -> bool {
        true
    }
}

fn serve(
    process: &PluginProcess,
    stdin: ChildStdin,
    control: &Receiver<JobControl>,
    streams: usize,
) -> i32 {
    let mut stdin = Some(stdin);
    let mut open_streams = streams;
    let mut connected = true;
    loop {
        if open_streams == 0 {
            if let Some(code) = process.try_wait() {
                return code;
            }
        }
        if !connected {
            thread::sleep(EXIT_POLL);
            continue;
        }
        let message = if open_streams == 0 {
            control.recv_timeout(EXIT_POLL)
        } else {
            control.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        match message {
            Ok(JobControl::Stdin(data)) => {
                if let Some(pipe) = stdin.as_mut() {
                    if pipe.write_all(&data).and_then(|()| pipe.flush()).is_err() {
                        stdin = None;
                    }
                }
            }
            Ok(JobControl::CloseStdin) => stdin = None,
            Ok(JobControl::StreamClosed) => open_streams = open_streams.saturating_sub(1),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                connected = false;
                stdin = None;
            }
        }
    }
}

fn spawn_reader(
    mut pipe: impl Read + Send + 'static,
    stream: JobStream,
    job: u64,
    id: usize,
    sender: &Sender<JobMessage>,
    control: &Sender<JobControl>,
) -> JoinHandle<()> {
    let sender = sender.clone();
    let control = control.clone();
    thread::spawn(move || {
        let mut chunk = vec![0; READ_CHUNK];
        let mut pending = Vec::new();
        loop {
            let read = match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            };
            pending.extend_from_slice(&chunk[..read]);
            let Some(end) = pending.iter().rposition(|&b| b == b'\n') else {
                continue;
            };
            let lines = split_lines(&pending[..end]);
            pending.drain(..=end);
            if !send_lines(&sender, id, job, stream, lines) {
                break;
            }
        }
        if !pending.is_empty() {
            send_lines(&sender, id, job, stream, split_lines(&pending));
        }
        let _ = control.send(JobControl::StreamClosed);
    })
}

fn split_lines(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|&b| b == b'\n')
        .map(|line| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            String::from_utf8_lossy(line).into_owned()
        })
        .collect()
}

fn send_lines(
    sender: &Sender<JobMessage>,
    id: usize,
    job: u64,
    stream: JobStream,
    lines: Vec<String>,
) -> bool {
    sender
        .send(JobMessage::Custom(
            id,
            Box::new(PluginJobPayload {
                job,
                event: PluginJobEvent::Output(stream, lines),
            }),
        ))
        .is_ok()
}
