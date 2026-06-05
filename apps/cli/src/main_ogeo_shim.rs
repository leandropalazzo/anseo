//! Back-compat shim: `ogeo` binary is deprecated; use `anseo` instead.
//!
//! This shim re-execs the real `anseo` binary (looked up next to itself so it
//! works whether installed globally or run from the Cargo `target/` directory).
//! If `anseo` is not found alongside `ogeo`, it falls back to re-running via
//! the same argv under the name `anseo` using `std::process::Command`.
//!
//! Deprecated since: 0.7.0 (Epic 45 rename). Will be removed in the next
//! major release.

fn main() {
    // Warn the user on stderr but don't block the command.
    eprintln!(
        "warning: `ogeo` is deprecated and will be removed in a future release. \
Use `anseo` instead."
    );

    // Attempt to exec the `anseo` binary that lives alongside this one.
    let anseo_path = std::env::current_exe()
        .expect("could not determine current executable path")
        .with_file_name("anseo");

    // On Windows the binary has an .exe extension.
    #[cfg(target_os = "windows")]
    anseo_path.set_extension("exe");

    let args: Vec<String> = std::env::args().skip(1).collect();
    let status = std::process::Command::new(&anseo_path)
        .args(&args)
        .status()
        .unwrap_or_else(|_| {
            // Fallback: try PATH
            std::process::Command::new("anseo")
                .args(&args)
                .status()
                .expect("failed to exec `anseo`; ensure it is installed and on PATH")
        });

    std::process::exit(status.code().unwrap_or(1));
}
