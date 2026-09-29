use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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
        let Some((program, rest)) = command.args.split_first() else {
            return Outcome::new(NOT_FOUND_EXIT, "", "no command given");
        };

        let mut child: Child = match std::process::Command::new(program)
            .args(rest)
            .current_dir(&command.cwd)
            .stdin(Stdio::piped())
            .stdout(stream_for(&command.stdout_file))
            .stderr(stream_for(&command.stderr_file))
            .spawn()
        {
            Ok(child) => child,
            Err(error) => return Outcome::new(NOT_FOUND_EXIT, "", error.to_string()),
        };

        if let Some(input) = &command.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(input.as_bytes());
            }
        } else {
            drop(child.stdin.take());
        }

        let stdout = child.stdout.take().map(read_async);
        let stderr = child.stderr.take().map(read_async);
        let status = wait_with_timeout(&mut child, command.timeout);

        let stdout = stdout
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();
        let stderr = stderr
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();

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
/// `nix develop` prints its shellHook and flake notices on stderr before the
/// tool runs, so its streams are captured in files rather than pipes — stdout
/// stays the tool's own, which is what every json payload the gates parse needs.
pub fn dev(runner: &dyn Runner, workdir: &Path, args: &[String], timeout: Option<u64>) -> Outcome {
    if runner.has("cargo") {
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

/// The last `count` lines of some output, trimmed.
pub fn last_lines(text: &str, count: usize) -> Vec<String> {
    let lines: Vec<&str> = text.trim().lines().collect();
    let start = lines.len().saturating_sub(count);
    lines[start..]
        .iter()
        .map(|line| (*line).trim().to_string())
        .collect()
}

pub fn git(runner: &dyn Runner, repo: &Path, args: &[&str]) -> String {
    let command_args = std::iter::once("git").chain(args.iter().copied());
    let result = runner.exec(&Command::new(repo, command_args));
    if result.ok() {
        result.stdout
    } else {
        String::new()
    }
}

/// The revision stamp: `git status --short | git hash-object --stdin`.
pub fn dirty_hash(runner: &dyn Runner, repo: &Path) -> String {
    let status = runner
        .exec(&Command::new(repo, ["git", "status", "--short"]))
        .stdout;
    runner
        .exec(&Command::new(repo, ["git", "hash-object", "--stdin"]).stdin(status))
        .stdout
        .trim()
        .to_string()
}

pub fn workspace_root(runner: &dyn Runner, cwd: &Path) -> PathBuf {
    let located = git(runner, cwd, &["rev-parse", "--show-toplevel"]);
    let trimmed = located.trim();
    if trimmed.is_empty() {
        cwd.to_path_buf()
    } else {
        PathBuf::from(trimmed)
    }
}

/// Repo-relative files changed against `base`, working tree included.
pub fn changed_files(runner: &dyn Runner, repo: &Path, base: &str) -> Vec<String> {
    let merge_base = git(runner, repo, &["merge-base", "HEAD", base]);
    let merge_base = {
        let trimmed = merge_base.trim();
        if trimmed.is_empty() {
            "HEAD".to_string()
        } else {
            trimmed.to_string()
        }
    };

    let committed = git(runner, repo, &["diff", "--name-only", &merge_base]);
    let staged = git(runner, repo, &["diff", "--name-only", "--cached"]);
    let untracked = git(
        runner,
        repo,
        &["ls-files", "--others", "--exclude-standard"],
    );

    let mut files: Vec<String> = [committed, staged, untracked]
        .iter()
        .flat_map(|output| output.lines().map(|line| line.to_string()))
        .filter(|line| !line.is_empty())
        .collect();
    files.sort();
    files.dedup();
    files
}

pub fn pick_base(runner: &dyn Runner, repo: &Path) -> String {
    for candidate in ["origin/mvp", "origin/develop", "origin/master"] {
        let result = runner.exec(&Command::new(
            repo,
            ["git", "rev-parse", "--verify", "--quiet", candidate],
        ));
        if result.ok() {
            return candidate.to_string();
        }
    }
    "HEAD".to_string()
}

#[cfg(test)]
mod tests;
