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

/// Requests UAC elevation ("runas") to relaunch the current executable as Administrator.
/// Returns `Ok(true)` if the elevated process was started, or `Ok(false)` if the user cancelled UAC.
pub fn relaunch_as_admin(working_dir: impl AsRef<Path>) -> Result<bool> {
    let current_exe = std::env::current_exe()
        .with_context(|| "Failed to resolve current executable path for elevation")?;
    let dir = working_dir.as_ref();

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
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

        let verb: Vec<u16> = std::ffi::OsStr::new("runas")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let file: Vec<u16> = current_exe
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let work_dir: Vec<u16> = dir
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let result = unsafe {
            ShellExecuteW(
                0,
                verb.as_ptr(),
                file.as_ptr(),
                ptr::null(),
                work_dir.as_ptr(),
                SW_SHOWNORMAL,
            )
        };

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
