use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::thread;
use std::time::{Duration, Instant};

mod git;

pub use git::{all_files, changed_files, dirty_hash, git, pick_base, workspace_root};

pub const TIMEOUT_EXIT: i32 = 124;
pub const NOT_FOUND_EXIT: i32 = 127;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub timeout: Option<u64>,
    pub stdin: Option<String>,
    pub stdout_file: Option<PathBuf>,
    pub stderr_file: Option<PathBuf>,
}

impl Command {
    pub fn new(cwd: impl Into<PathBuf>, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            args: args.into_iter().map(Into::into).collect(),
            cwd: cwd.into(),
            timeout: None,
            stdin: None,
            stdout_file: None,
            stderr_file: None,
        }
    }

    pub fn timeout(mut self, timeout: Option<u64>) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn stdin(mut self, input: impl Into<String>) -> Self {
        self.stdin = Some(input.into());
        self
    }

    /// Send the process's stdout to a file instead of a pipe.
    pub fn stdout_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.stdout_file = Some(path.into());
        self
    }

    pub fn stderr_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.stderr_file = Some(path.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Outcome {
    pub fn new(code: i32, stdout: impl Into<String>, stderr: impl Into<String>) -> Self {
        Self {
            code,
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }

    pub fn ok(&self) -> bool {
        self.code == 0
    }

    pub fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

pub trait Runner {
    fn exec(&self, command: &Command) -> Outcome;
    fn has(&self, tool: &str) -> bool;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemRunner;

impl Runner for SystemRunner {
    fn exec(&self, command: &Command) -> Outcome {
        let mut child = match spawn(command) {
            Ok(child) => child,
            Err(outcome) => return outcome,
        };
        feed_stdin(&mut child, command.stdin.as_ref());

        let stdout = child.stdout.take().map(read_async);
        let stderr = child.stderr.take().map(read_async);
        let status = wait_with_timeout(&mut child, command.timeout);

        let stdout = collect(stdout);
        let stderr = collect(stderr);
        match status {
            Some(status) => Outcome::new(status, stdout, stderr),
            None => Outcome::new(
                TIMEOUT_EXIT,
                stdout,
                format!("timed out after {}s", command.timeout.unwrap_or_default()),
            ),
        }
    }

    fn has(&self, tool: &str) -> bool {
        let Some(path) = std::env::var_os("PATH") else {
            return false;
        };
        std::env::split_paths(&path).any(|directory| is_executable(&directory.join(tool)))
    }
}

fn spawn(command: &Command) -> Result<Child, Outcome> {
    let Some((program, rest)) = command.args.split_first() else {
        return Err(Outcome::new(NOT_FOUND_EXIT, "", "no command given"));
    };

    std::process::Command::new(program)
        .args(rest)
        .current_dir(&command.cwd)
        .stdin(Stdio::piped())
        .stdout(stream_for(&command.stdout_file))
        .stderr(stream_for(&command.stderr_file))
        .spawn()
        .map_err(|error| Outcome::new(NOT_FOUND_EXIT, "", error.to_string()))
}

/// Hand the child its input and close the pipe when there is none.
fn feed_stdin(child: &mut Child, input: Option<&String>) {
    if let Some(input) = input {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(input.as_bytes());
        }
    } else {
        drop(child.stdin.take());
    }
}

fn collect(handle: Option<thread::JoinHandle<String>>) -> String {
    handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default()
}

/// A file the caller asked for, or a pipe when the file cannot be created.
fn stream_for(file: &Option<PathBuf>) -> Stdio {
    match file {
        Some(path) => match fs::File::create(path) {
            Ok(handle) => Stdio::from(handle),
            Err(_) => Stdio::piped(),
        },
        None => Stdio::piped(),
    }
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn read_async(mut stream: impl Read + Send + 'static) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut buffer = String::new();
        let _ = stream.read_to_string(&mut buffer);
        buffer
    })
}

fn wait_with_timeout(child: &mut Child, timeout: Option<u64>) -> Option<i32> {
    let Some(seconds) = timeout else {
        return child.wait().ok().map(exit_status);
    };

    let deadline = Instant::now() + Duration::from_secs(seconds);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(exit_status(status)),
            Ok(None) => {}
            Err(_) => return None,
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn exit_status(status: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

#[cfg(not(unix))]
fn exit_status(status: std::process::ExitStatus) -> i32 {
    status.code().unwrap_or(-1)
}

/// Run a command with `workdir` as its directory, inside the dev shell when needed.
///
/// The language module names the binary (`Lang::env_tool`) whose presence means
/// the environment already carries the gate tools; when it is missing the command
/// runs through `nix develop -c …`.
///
/// `nix develop` prints its shellHook and flake notices on stderr before the
/// tool runs, so its streams are captured in files rather than pipes — stdout
/// stays the tool's own, which is what every json payload the gates parse needs.
pub fn dev(
    runner: &dyn Runner,
    probe_tool: &str,
    workdir: &Path,
    args: &[String],
    timeout: Option<u64>,
) -> Outcome {
    if runner.has(probe_tool) {
        return runner.exec(&Command::new(workdir, args.to_vec()).timeout(timeout));
    }

    let directory = scratch_directory("guardrails-dev-");
    let out_path = directory.join("stdout");
    let err_path = directory.join("stderr");
    let mut argv = vec!["nix".to_string(), "develop".to_string(), "-c".to_string()];
    argv.extend(args.iter().cloned());
    let result = runner.exec(
        &Command::new(workdir, argv)
            .timeout(timeout)
            .stdout_file(out_path.clone())
            .stderr_file(err_path.clone()),
    );

    let stdout = fs::read_to_string(&out_path).unwrap_or(result.stdout);
    let stderr = fs::read_to_string(&err_path).unwrap_or(result.stderr);
    let _ = fs::remove_dir_all(&directory);
    Outcome::new(result.code, stdout, stderr)
}

pub fn scratch_directory(prefix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let directory = std::env::temp_dir().join(format!("{prefix}{}-{stamp}", std::process::id()));
    let _ = fs::create_dir_all(&directory);
    directory
}

/// The last `count` lines of some output, trimmed and prefixed.
pub fn last_lines_with(text: &str, count: usize, prefix: &str) -> Vec<String> {
    let lines: Vec<&str> = text.trim().lines().collect();
    let start = lines.len().saturating_sub(count);
    lines[start..]
        .iter()
        .map(|line| format!("{prefix}{}", line.trim()))
        .collect()
}

/// The last `count` lines of some output, trimmed.
pub fn last_lines(text: &str, count: usize) -> Vec<String> {
    last_lines_with(text, count, "")
}

#[cfg(test)]
mod tests;
