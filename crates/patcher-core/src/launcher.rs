use std::path::Path;
use std::process::Command;
use anyhow::{bail, Context, Result};

pub fn launch_game(
    executable_path: impl AsRef<Path>,
    arguments: &[String],
    working_dir: impl AsRef<Path>,
) -> Result<()> {
    let exe = executable_path.as_ref();
    let dir = working_dir.as_ref();

    let full_exe = if exe.is_absolute() {
        exe.to_path_buf()
    } else {
        dir.join(exe)
    };

    if !full_exe.exists() {
        bail!("Client executable not found at {:?}", full_exe);
    }

    log::info!("Launching client: {:?} with args {:?}", full_exe, arguments);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS flag (0x00000008) so the game runs independently of the patcher
        const DETACHED_PROCESS: u32 = 0x00000008;

        let mut cmd = Command::new(&full_exe);
        cmd.args(arguments)
            .current_dir(dir)
            .creation_flags(DETACHED_PROCESS);

        cmd.spawn()
            .with_context(|| format!("Failed to launch executable {:?}", full_exe))?;
    }

    #[cfg(not(windows))]
    {
        let mut cmd = Command::new(&full_exe);
        cmd.args(arguments)
            .current_dir(dir);

        cmd.spawn()
            .with_context(|| format!("Failed to launch executable {:?}", full_exe))?;
    }

    Ok(())
}

pub fn launch_setup(
    setup_executable: &str,
    working_dir: impl AsRef<Path>,
) -> Result<()> {
    let dir = working_dir.as_ref();
    let full_exe = dir.join(setup_executable);

    if !full_exe.exists() {
        bail!("Setup executable not found at {:?}", full_exe);
    }

    log::info!("Launching setup: {:?}", full_exe);

    Command::new(&full_exe)
        .current_dir(dir)
        .spawn()
        .with_context(|| format!("Failed to launch setup {:?}", full_exe))?;

    Ok(())
}

/// Cleans up leftover `<exe>.old` backup file created during a previous self-update.
pub fn cleanup_self_update_backup() {
    if let Ok(mut exe_path) = std::env::current_exe() {
        exe_path.set_extension("exe.old");
        if exe_path.exists() {
            let _ = std::fs::remove_file(&exe_path);
        }
    }
}

/// Spawns a new instance of the currently running executable (after self-update).
pub fn restart_current_exe(working_dir: impl AsRef<Path>) -> Result<()> {
    let current_exe = std::env::current_exe()
        .with_context(|| "Failed to resolve current executable path for restart")?;
    let dir = working_dir.as_ref();

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;

        Command::new(&current_exe)
            .current_dir(dir)
            .creation_flags(DETACHED_PROCESS)
            .spawn()
            .with_context(|| format!("Failed to restart patcher {:?}", current_exe))?;
    }

    #[cfg(not(windows))]
    {
        Command::new(&current_exe)
            .current_dir(dir)
            .spawn()
            .with_context(|| format!("Failed to restart patcher {:?}", current_exe))?;
    }

    Ok(())
}

/// Checks whether the current process is running with elevated Administrator privileges.
pub fn is_running_as_admin() -> bool {
    #[cfg(windows)]
    {
        #[link(name = "shell32")]
        extern "system" {
            fn IsUserAnAdmin() -> i32;
        }
        unsafe { IsUserAnAdmin() != 0 }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
fn to_wide_null(s: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn shell_execute_w(
    verb: &str,
    file: &std::ffi::OsStr,
    working_dir: Option<&std::ffi::OsStr>,
) -> isize {
    use std::ptr;

    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: isize,
            lp_operation: *const u16,
            lp_file: *const u16,
            lp_parameters: *const u16,
            lp_directory: *const u16,
            n_show_cmd: i32,
        ) -> isize;
    }

    const SW_SHOWNORMAL: i32 = 1;

    let verb_w = to_wide_null(std::ffi::OsStr::new(verb));
    let file_w = to_wide_null(file);
    let dir_w = working_dir.map(to_wide_null);
    let dir_ptr = dir_w.as_ref().map_or(ptr::null(), |d| d.as_ptr());

    unsafe {
        ShellExecuteW(
            0,
            verb_w.as_ptr(),
            file_w.as_ptr(),
            ptr::null(),
            dir_ptr,
            SW_SHOWNORMAL,
        )
    }
}

/// Validates that a URL is a well-formed HTTP or HTTPS link safe to pass to the OS shell.
pub fn validate_external_url(url: &str) -> Result<&str> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        bail!("External URL cannot be empty");
    }
    if trimmed.chars().any(|c| c.is_control()) {
        bail!("External URL contains invalid control characters");
    }

    let lower = trimmed.to_ascii_lowercase();
    let after_scheme = if let Some(rest) = lower.strip_prefix("https://") {
        rest
    } else if let Some(rest) = lower.strip_prefix("http://") {
        rest
    } else {
        bail!("Only http:// and https:// URLs are allowed: {:?}", trimmed);
    };

    if after_scheme.is_empty() || after_scheme.starts_with('/') {
        bail!("External URL is missing a valid host: {:?}", trimmed);
    }

    Ok(trimmed)
}

/// Opens an external HTTP(S) URL in the user's default web browser.
pub fn open_external_url(url: &str) -> Result<()> {
    let valid_url = validate_external_url(url)?;
    log::info!("Opening external URL in default browser: {}", valid_url);

    #[cfg(windows)]
    {
        let result = shell_execute_w("open", std::ffi::OsStr::new(valid_url), None);
        // ShellExecuteW returns a value > 32 on success.
        if result <= 32 {
            bail!(
                "ShellExecuteW failed to open URL {:?} (error code {})",
                valid_url,
                result
            );
        }
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(valid_url)
            .spawn()
            .with_context(|| format!("Failed to launch browser for {:?}", valid_url))?;
    }

    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(valid_url)
            .spawn()
            .with_context(|| format!("Failed to launch browser for {:?}", valid_url))?;
    }

    Ok(())
}

/// Requests UAC elevation ("runas") to relaunch the current executable as Administrator.
/// Returns `Ok(true)` if the elevated process was started, or `Ok(false)` if the user cancelled UAC.
pub fn relaunch_as_admin(working_dir: impl AsRef<Path>) -> Result<bool> {
    let current_exe = std::env::current_exe()
        .with_context(|| "Failed to resolve current executable path for elevation")?;
    let dir = working_dir.as_ref();

    #[cfg(windows)]
    {
        let result = shell_execute_w(
            "runas",
            current_exe.as_os_str(),
            Some(dir.as_os_str()),
        );

        // ShellExecuteW returns a value > 32 on success.
        // <= 32 indicates an error or user cancellation (e.g., SE_ERR_ACCESSDENIED = 5).
        Ok(result > 32)
    }

    #[cfg(not(windows))]
    {
        let _ = (current_exe, dir);
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_external_url_accepts_http_and_https() {
        assert_eq!(
            validate_external_url("https://discord.gg").unwrap(),
            "https://discord.gg"
        );
        assert_eq!(
            validate_external_url("  http://example.com/news?id=1  ").unwrap(),
            "http://example.com/news?id=1"
        );
        assert_eq!(
            validate_external_url("HTTPS://pronterabox.com/events").unwrap(),
            "HTTPS://pronterabox.com/events"
        );
    }

    #[test]
    fn test_validate_external_url_rejects_unsafe_or_malformed() {
        for bad in [
            "",
            "   ",
            "#",
            "javascript:alert(1)",
            "file:///C:/Windows/System32/calc.exe",
            "cmd://whoami",
            "ms-msdt:id",
            "https://",
            "http:///path",
            "https://example.com\nfoo",
        ] {
            assert!(
                validate_external_url(bad).is_err(),
                "Expected {:?} to be rejected",
                bad
            );
            assert!(
                open_external_url(bad).is_err(),
                "Expected open_external_url({:?}) to return Err",
                bad
            );
        }
    }
}
