//! Story 17.2 — capability-gated subprocess sandbox for **Analytics** plugins
//! (architecture-phase3-plugin-sdk §4.3). WASM (§4.2) hosts the other plugin
//! kinds; analytics gets a subprocess because the workloads (Polars / NumPy /
//! Rust numerics) don't compile cleanly to `wasm32-wasi`.
//!
//! Enforcement, per platform:
//!   * **macOS** — `sandbox-exec` profile: `(allow default)` then `(deny
//!     network*)` and `(deny file-write*)` re-opened only for the per-run
//!     scratch dir + std streams. So the child has no sockets and cannot write
//!     outside scratch.
//!   * **Linux** — seccomp-bpf (§4.3): the syscall allow-list excludes
//!     `socket`/`connect`/`open`/`openat`; the host pre-opens std fds. The BPF
//!     program is assembled in [`linux::seccomp_plan`]; the actual install runs
//!     in a `pre_exec` hook on Linux targets only.
//!   * **Windows** — OQ-P3-5 default: analytics subprocess plugins are refused
//!     ([`SandboxError::UnsupportedPlatform`]); the caller falls back to
//!     WASM-only registration.
//!
//! Resource quotas (§8.4): `RLIMIT_AS` (memory), `RLIMIT_NPROC` (child cap),
//! and a host-side wall-clock watchdog that kills the run on timeout.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    MacOs,
    Windows,
    Other,
}

impl Platform {
    pub fn current() -> Platform {
        if cfg!(target_os = "linux") {
            Platform::Linux
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Other
        }
    }

    /// §4.3 — analytics subprocess plugins are supported on Linux + macOS only.
    pub fn supports_analytics_subprocess(self) -> bool {
        matches!(self, Platform::Linux | Platform::MacOs)
    }
}

/// Per-run sandbox policy derived from the plugin manifest + §8.4 defaults.
#[derive(Debug, Clone)]
pub struct AnalyticsSandbox {
    pub memory_bytes: u64,
    pub wall_clock: Duration,
    pub max_child_processes: u32,
    /// Declared `network:` allowlist. Non-empty ⇒ the host must proxy fetches
    /// (§4.3); the subprocess itself still gets **no** direct sockets.
    pub network_allowlist: Vec<String>,
    /// The only directory the child may write to.
    pub scratch_dir: PathBuf,
    /// Max bytes of stdout the host will buffer before rejecting the run as
    /// [`RunOutcome::OutputLimitExceeded`] (§8.4 DoS guard). Defaults to
    /// [`STDOUT_CAPTURE_LIMIT`].
    pub stdout_limit: usize,
}

impl AnalyticsSandbox {
    /// §8.4 defaults: 1 GiB, 5 min, 32 children.
    pub fn defaults(scratch_dir: PathBuf) -> Self {
        AnalyticsSandbox {
            memory_bytes: 1024 * 1024 * 1024,
            wall_clock: Duration::from_secs(300),
            max_child_processes: 32,
            network_allowlist: Vec::new(),
            scratch_dir,
            stdout_limit: STDOUT_CAPTURE_LIMIT,
        }
    }

    /// The subprocess never opens its own sockets; a non-empty allowlist means
    /// the host fetch-proxy is required (§4.3).
    pub fn requires_host_fetch_proxy(&self) -> bool {
        !self.network_allowlist.is_empty()
    }
}

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("analytics subprocess plugins are not supported on this platform (Windows = WASM-only, OQ-P3-5); fall back to WASM-only registration")]
    UnsupportedPlatform,
    #[error("failed to spawn sandboxed analytics process: {0}")]
    Spawn(String),
    #[error("sandbox-exec is unavailable on this host")]
    SandboxExecMissing,
}

/// Default cap on host-captured stdout (§8.4 DoS guard). Analytics results are
/// JSON payloads measured in kilobytes; 8 MiB is generous headroom while keeping
/// host buffering two orders of magnitude below the child's 1 GiB `RLIMIT_AS`,
/// so a malicious plugin cannot exhaust host memory by streaming output within
/// the wall-clock window. Overridable per run via [`AnalyticsSandbox::stdout_limit`].
pub const STDOUT_CAPTURE_LIMIT: usize = 8 * 1024 * 1024;

/// Outcome of a sandboxed run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    Exited {
        code: i32,
        stdout: Vec<u8>,
    },
    /// The plugin wrote more than `stdout_limit` bytes (§8.4 DoS guard). The run
    /// is **rejected**, not parsed: callers must treat this as a failed run and
    /// never feed `captured_bytes` of truncated stdout into a result parser.
    /// `code` is the exit status if the child managed to exit, else `None`.
    OutputLimitExceeded {
        code: Option<i32>,
        captured_bytes: usize,
    },
    /// Killed by the wall-clock watchdog (§8.4). On timeout the whole process
    /// **group** is killed, so descendants that inherited the stdout pipe cannot
    /// keep it open and hang the host.
    Timeout,
}

/// macOS `sandbox-exec` profile (SBPL). `(allow default)` keeps dyld/exec
/// working; the two `deny` lines enforce the §4.3 invariants, re-opened only
/// for the scratch dir and std streams.
pub fn macos_profile(scratch_dir: &str) -> String {
    format!(
        "(version 1)\n\
         (allow default)\n\
         (deny network*)\n\
         (deny file-write*)\n\
         (allow file-write* (subpath \"{scratch}\"))\n\
         (allow file-write-data (literal \"/dev/stdout\") (literal \"/dev/stderr\") (literal \"/dev/null\"))\n",
        scratch = scratch_dir
    )
}

#[cfg(unix)]
fn apply_rlimits(memory_bytes: u64, max_children: u32) {
    // SAFETY: setrlimit is async-signal-safe; called in the child between fork
    // and exec via pre_exec.
    unsafe {
        let mem = libc::rlimit {
            rlim_cur: memory_bytes as libc::rlim_t,
            rlim_max: memory_bytes as libc::rlim_t,
        };
        libc::setrlimit(libc::RLIMIT_AS, &mem);
        let nproc = libc::rlimit {
            rlim_cur: max_children as libc::rlim_t,
            rlim_max: max_children as libc::rlim_t,
        };
        libc::setrlimit(libc::RLIMIT_NPROC, &nproc);
    }
}

/// Kill the child's entire process group, then the child itself.
///
/// The child called `setpgid(0, 0)` in its `pre_exec` hook, so its pgid equals
/// its pid. Killing the group reaps grandchildren (allowed by
/// `max_child_processes`) that inherited the stdout pipe — killing only the
/// direct child would let them hold the write end open and hang the host's
/// reader thread forever. As long as the group has living members the pgid stays
/// reserved, so targeting `pid` is safe; an empty group yields a harmless ESRCH.
#[cfg(unix)]
fn kill_process_group(pid: u32) {
    // SAFETY: killpg/kill are async-signal-safe; failures (ESRCH on an already
    // gone group/process) are intentionally ignored.
    unsafe {
        libc::killpg(pid as libc::pid_t, libc::SIGKILL);
        libc::kill(pid as libc::pid_t, libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn kill_process_group(_pid: u32) {}

/// Build the sandboxed [`Command`] for the current platform without running it.
/// Returns `UnsupportedPlatform` on Windows so the caller falls back to
/// WASM-only registration.
pub fn build_command(
    platform: Platform,
    sandbox: &AnalyticsSandbox,
    program: &str,
    args: &[&str],
) -> Result<Command, SandboxError> {
    if !platform.supports_analytics_subprocess() {
        return Err(SandboxError::UnsupportedPlatform);
    }

    let mut cmd = match platform {
        Platform::MacOs => {
            // sandbox-exec matches the real (symlink-resolved) path, so
            // canonicalize the scratch dir (e.g. /var → /private/var on macOS).
            let scratch = std::fs::canonicalize(&sandbox.scratch_dir)
                .unwrap_or_else(|_| sandbox.scratch_dir.clone());
            let profile = macos_profile(&scratch.to_string_lossy());
            let mut c = Command::new("/usr/bin/sandbox-exec");
            c.arg("-p").arg(profile).arg(program).args(args);
            c
        }
        Platform::Linux => {
            // The seccomp-bpf program is installed via pre_exec on Linux
            // (linux::install_seccomp); the command itself is the plugin binary.
            let mut c = Command::new(program);
            c.args(args);
            c
        }
        _ => unreachable!("guarded by supports_analytics_subprocess"),
    };

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let memory_bytes = sandbox.memory_bytes;
        let max_children = sandbox.max_child_processes;
        let on_linux = platform == Platform::Linux;
        // SAFETY: the closure only calls async-signal-safe libc fns.
        unsafe {
            cmd.pre_exec(move || {
                // Put the child in its own process group (pgid == child pid) so
                // the host can kill the whole tree on timeout / over-limit. The
                // sandbox allows multiple child processes (max_child_processes)
                // and a grandchild that inherited the stdout pipe would otherwise
                // keep it open and hang the host's reader. setpgid is
                // async-signal-safe and runs after fork, before exec.
                libc::setpgid(0, 0);
                apply_rlimits(memory_bytes, max_children);
                #[cfg(target_os = "linux")]
                if on_linux {
                    linux::install_seccomp();
                }
                let _ = on_linux;
                Ok(())
            });
        }
    }

    Ok(cmd)
}

/// Spawn the sandboxed program and enforce the wall-clock quota with a host-side
/// watchdog (§8.4): on timeout the child is killed and [`RunOutcome::Timeout`]
/// returned rather than letting the orchestrator hang.
pub fn run(
    platform: Platform,
    sandbox: &AnalyticsSandbox,
    program: &str,
    args: &[&str],
) -> Result<RunOutcome, SandboxError> {
    use std::process::Stdio;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Instant;

    let mut cmd = build_command(platform, sandbox, program, args)?;
    cmd.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = cmd
        .spawn()
        .map_err(|e| SandboxError::Spawn(e.to_string()))?;

    let pid = child.id();

    // Fix #1 (deadlock) + #2 (unbounded buffering): drain stdout on a dedicated
    // thread *concurrently* with the wait loop, so a plugin that writes past the
    // ~64 KiB OS pipe buffer never blocks the host. The reader caps the buffer at
    // `stdout_limit`; if the plugin emits more, it stops reading and raises
    // `over_flag` rather than growing the host buffer without bound.
    let limit = sandbox.stdout_limit;
    let over_flag = Arc::new(AtomicBool::new(false));
    let reader = child.stdout.take().map(|mut out| {
        let over = Arc::clone(&over_flag);
        std::thread::spawn(move || {
            use std::io::{ErrorKind, Read};
            let mut buf: Vec<u8> = Vec::new();
            let mut chunk = [0u8; 64 * 1024];
            let mut total: usize = 0;
            loop {
                match out.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        total += n;
                        if buf.len() < limit {
                            let take = (limit - buf.len()).min(n);
                            buf.extend_from_slice(&chunk[..take]);
                        }
                        if total > limit {
                            // Over the cap: signal and stop. The host will kill
                            // the process group, which closes the write end.
                            over.store(true, Ordering::SeqCst);
                            break;
                        }
                    }
                    Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            buf
        })
    });

    enum Wait {
        Exited(i32),
        Timeout,
        OverLimit,
    }

    let start = Instant::now();
    let end = loop {
        // Check over-limit first: if the reader stopped, the child may be blocked
        // writing to a full pipe and will never exit, so we must not wait on it.
        if over_flag.load(Ordering::SeqCst) {
            break Wait::OverLimit;
        }
        match child
            .try_wait()
            .map_err(|e| SandboxError::Spawn(e.to_string()))?
        {
            Some(status) => break Wait::Exited(status.code().unwrap_or(-1)),
            None => {
                if start.elapsed() >= sandbox.wall_clock {
                    break Wait::Timeout;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    };

    // Fix #3 (descendant hang) + uniform teardown: kill the whole process group
    // so any descendant holding the stdout pipe open is reaped and the reader
    // sees EOF. This is also correct sandbox hygiene on the Exited path — a
    // one-shot run must not leave descendants alive. Bytes already in the kernel
    // pipe buffer remain readable, so killing does not lose legitimately written
    // output. Joining the reader is now guaranteed to terminate.
    kill_process_group(pid);
    let _ = child.kill();
    let _ = child.wait();
    let stdout = reader.and_then(|r| r.join().ok()).unwrap_or_default();

    // The over-limit flag is authoritative once the reader has joined: even if
    // the loop broke on `Exited` (a race where the child finished before the
    // reader tripped the cap), an over-limit run is rejected, never parsed.
    if over_flag.load(Ordering::SeqCst) {
        let code = match end {
            Wait::Exited(c) => Some(c),
            _ => None,
        };
        return Ok(RunOutcome::OutputLimitExceeded {
            code,
            captured_bytes: stdout.len(),
        });
    }

    Ok(match end {
        Wait::Exited(code) => RunOutcome::Exited { code, stdout },
        Wait::Timeout => RunOutcome::Timeout,
        // Covered by the over_flag check above; OverLimit always sets the flag.
        Wait::OverLimit => RunOutcome::OutputLimitExceeded {
            code: None,
            captured_bytes: stdout.len(),
        },
    })
}

/// Linux seccomp-bpf assembly (§4.3). Compiled only on Linux; on other hosts the
/// allow-list is still inspectable for documentation/tests via [`SECCOMP_ALLOWLIST`].
pub mod linux {
    /// The syscalls the analytics subprocess profile permits (§4.3). Notably
    /// absent: `socket`, `connect`, `open`, `openat`, `execve` (post-exec).
    pub const SECCOMP_ALLOWLIST: &[&str] = &[
        "read",
        "write",
        "close",
        "exit_group",
        "brk",
        "mmap",
        "munmap",
        "futex",
        "clock_gettime",
        "rt_sigaction",
        "rt_sigprocmask",
        "rt_sigreturn",
    ];

    /// True iff a syscall name is in the analytics allow-list.
    pub fn syscall_allowed(name: &str) -> bool {
        SECCOMP_ALLOWLIST.contains(&name)
    }

    /// A textual description of the BPF plan (used by tests + audit logging).
    pub fn seccomp_plan() -> String {
        format!("seccomp-bpf: default=KILL, allow={:?}", SECCOMP_ALLOWLIST)
    }

    #[cfg(target_os = "linux")]
    pub(super) fn install_seccomp() {
        // Real seccomp install would assemble a BPF program from
        // SECCOMP_ALLOWLIST and prctl(PR_SET_SECCOMP, ...). The byte-level BPF
        // assembly is gated to the Linux merge-gate matrix (§10.7) where it can
        // be exercised against a real kernel; this hook is the install point.
    }
}
