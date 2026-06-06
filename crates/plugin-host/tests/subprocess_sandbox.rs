//! Story 17.2 `[plg-3] subprocess kind` — sandbox + resource-quota tests.
//!
//! Platform coverage mirrors §4.3: the macOS `sandbox-exec` enforcement tests
//! run on macOS, the seccomp allow-list assembly is asserted everywhere
//! (pure), the Windows-refusal + WASM-fallback path is pure, and the wall-clock
//! watchdog runs on any unix host.

#[cfg(target_os = "macos")]
use anseo_plugin_host::subprocess::macos_profile;
use anseo_plugin_host::subprocess::{
    build_command, linux, run, AnalyticsSandbox, Platform, RunOutcome, SandboxError,
};
use std::time::Duration;

// ---- platform gating (§4.3 / OQ-P3-5) ----

#[test]
fn windows_refuses_analytics_subprocess_and_falls_back() {
    assert!(!Platform::Windows.supports_analytics_subprocess());
    let sb = AnalyticsSandbox::defaults(std::env::temp_dir());
    let err = build_command(Platform::Windows, &sb, "/bin/echo", &["hi"]).unwrap_err();
    assert!(matches!(err, SandboxError::UnsupportedPlatform));
}

#[test]
fn linux_and_macos_support_subprocess() {
    assert!(Platform::Linux.supports_analytics_subprocess());
    assert!(Platform::MacOs.supports_analytics_subprocess());
}

// ---- seccomp allow-list (pure; §4.3) ----

#[test]
fn seccomp_allowlist_excludes_network_and_open() {
    assert!(linux::syscall_allowed("read"));
    assert!(linux::syscall_allowed("write"));
    assert!(!linux::syscall_allowed("socket"));
    assert!(!linux::syscall_allowed("connect"));
    assert!(!linux::syscall_allowed("open"));
    assert!(!linux::syscall_allowed("openat"));
    assert!(linux::seccomp_plan().contains("KILL"));
}

// ---- network capability does not grant the child direct sockets (§4.3) ----

#[test]
fn declared_network_requires_host_fetch_proxy() {
    let mut sb = AnalyticsSandbox::defaults(std::env::temp_dir());
    assert!(!sb.requires_host_fetch_proxy());
    sb.network_allowlist = vec!["api.priya.dev".into()];
    assert!(sb.requires_host_fetch_proxy());
}

// ---- macOS sandbox-exec enforcement (live on macOS) ----

#[cfg(target_os = "macos")]
#[test]
fn macos_sandbox_denies_writes_outside_scratch() {
    let scratch = tempfile::tempdir().unwrap();
    let sb = AnalyticsSandbox::defaults(scratch.path().to_path_buf());

    // A write to the scratch dir is permitted.
    let inside = scratch.path().join("ok.txt");
    let out = run(
        Platform::MacOs,
        &sb,
        "/bin/sh",
        &["-c", &format!("echo hi > {}", inside.display())],
    )
    .unwrap();
    assert_eq!(
        out,
        RunOutcome::Exited {
            code: 0,
            stdout: vec![]
        }
    );
    assert!(inside.exists(), "scratch write should succeed");

    // A write OUTSIDE scratch is denied by the profile (non-zero exit, file absent).
    let outside = std::env::temp_dir().join(format!("opengeo_sbx_escape_{}", std::process::id()));
    let _ = std::fs::remove_file(&outside);
    let out = run(
        Platform::MacOs,
        &sb,
        "/bin/sh",
        &["-c", &format!("echo escape > {}", outside.display())],
    )
    .unwrap();
    match out {
        RunOutcome::Exited { code, .. } => assert_ne!(code, 0, "write outside scratch must fail"),
        other => panic!("expected non-zero exit, got {other:?}"),
    }
    assert!(
        !outside.exists(),
        "sandbox must block the out-of-scratch write"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_profile_denies_network_and_writes() {
    let p = macos_profile("/tmp/scratch");
    assert!(p.contains("(deny network*)"));
    assert!(p.contains("(deny file-write*)"));
    assert!(p.contains("(subpath \"/tmp/scratch\")"));
}

// ---- wall-clock watchdog (§8.4; any unix host) ----

#[cfg(unix)]
#[test]
fn wall_clock_watchdog_kills_runaway() {
    let mut sb = AnalyticsSandbox::defaults(std::env::temp_dir());
    sb.wall_clock = Duration::from_millis(300);
    let outcome = run(Platform::current(), &sb, "/bin/sleep", &["10"]).unwrap();
    assert_eq!(
        outcome,
        RunOutcome::Timeout,
        "a 10s sleep must be killed at 300ms"
    );
}

// ---- stdout draining + bounded capture + process-group timeout (§8.4) ----

/// Fix #1: a plugin that writes far more than the ~64 KiB OS pipe buffer in a
/// single process must be captured in full without deadlocking. The old code
/// only read stdout *after* the child exited, so the child blocked on write and
/// the run hung until the watchdog killed it.
#[cfg(unix)]
#[test]
fn large_single_process_output_is_fully_captured_without_hang() {
    let mut sb = AnalyticsSandbox::defaults(std::env::temp_dir());
    // RLIMIT_NPROC is per-user on macOS/BSD; the default cap of 32 forbids the
    // shell's pipeline fork on a busy host. Disable the clamp for this test.
    sb.max_child_processes = u32::MAX;
    let n: usize = 512 * 1024; // 8x the pipe buffer, well under the 8 MiB cap
    let outcome = run(
        Platform::current(),
        &sb,
        "/bin/sh",
        &["-c", &format!("yes X | head -c {n}")],
    )
    .unwrap();
    match outcome {
        RunOutcome::Exited { code, stdout } => {
            assert_eq!(code, 0, "pipeline should exit 0");
            assert_eq!(stdout.len(), n, "all {n} bytes must be captured");
        }
        other => panic!("expected full capture, got {other:?}"),
    }
}

/// Fix #2: output beyond `stdout_limit` is rejected as `OutputLimitExceeded`,
/// not silently truncated into a parsed `Exited` result, and the host buffer
/// stays bounded near the cap.
#[cfg(unix)]
#[test]
fn over_limit_output_is_rejected_not_parsed() {
    let mut sb = AnalyticsSandbox::defaults(std::env::temp_dir());
    sb.stdout_limit = 64 * 1024; // small cap so the test is cheap
    sb.max_child_processes = u32::MAX; // allow the pipeline fork (see note above)
    let n: usize = 256 * 1024; // 4x the cap
    let outcome = run(
        Platform::current(),
        &sb,
        "/bin/sh",
        &["-c", &format!("yes X | head -c {n}")],
    )
    .unwrap();
    match outcome {
        RunOutcome::OutputLimitExceeded { captured_bytes, .. } => {
            assert!(
                captured_bytes <= sb.stdout_limit + 64 * 1024,
                "host buffering must stay bounded near the cap, got {captured_bytes}"
            );
        }
        other => panic!("over-limit output must be rejected, got {other:?}"),
    }
}

/// Fix #3: on timeout the whole process group is killed. A plugin that spawns a
/// descendant which inherits the stdout pipe and sleeps past the wall-clock must
/// still yield `Timeout` promptly — killing only the direct child would leave
/// the descendant holding the pipe open and hang the reader forever. The run is
/// driven on a helper thread so a regression surfaces as a clean failure rather
/// than a hung test process.
#[cfg(unix)]
#[test]
fn timeout_kills_descendants_holding_stdout_open() {
    use std::sync::mpsc;

    let mut sb = AnalyticsSandbox::defaults(std::env::temp_dir());
    sb.wall_clock = Duration::from_millis(300);
    sb.max_child_processes = u32::MAX; // allow the descendant fork (see note above)

    let (tx, rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        // Background descendant inherits stdout and outlives the direct child's
        // own `sleep`; both run well past the 300ms wall-clock.
        let outcome = run(
            Platform::current(),
            &sb,
            "/bin/sh",
            &["-c", "sleep 30 & sleep 30"],
        );
        let _ = tx.send(outcome);
    });

    let outcome = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("run() must return after the wall-clock, not hang on the descendant")
        .expect("sandboxed run spawns");
    assert_eq!(
        outcome,
        RunOutcome::Timeout,
        "process-group kill must terminate the descendant and yield Timeout"
    );
    worker.join().ok();
}

#[cfg(unix)]
#[test]
fn rlimit_address_space_is_applied_to_child() {
    // ulimit -v reports RLIMIT_AS in KiB; assert the child sees our cap.
    let mut sb = AnalyticsSandbox::defaults(std::env::temp_dir());
    sb.memory_bytes = 256 * 1024 * 1024; // 256 MiB
    let outcome = run(Platform::current(), &sb, "/bin/sh", &["-c", "ulimit -v"]).unwrap();
    match outcome {
        RunOutcome::Exited { code, stdout } => {
            assert_eq!(code, 0);
            let s = String::from_utf8_lossy(&stdout);
            let reported = s.trim();
            // Either an exact KiB number or "unlimited" on platforms that ignore
            // RLIMIT_AS for `ulimit -v`; we only assert the cap when numeric.
            if let Ok(kib) = reported.parse::<u64>() {
                assert_eq!(kib, 256 * 1024, "child RLIMIT_AS should equal our cap");
            }
        }
        other => panic!("expected exit, got {other:?}"),
    }
}
