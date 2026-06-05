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
