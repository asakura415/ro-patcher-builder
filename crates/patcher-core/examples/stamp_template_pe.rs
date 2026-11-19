use std::path::Path;
use anyhow::{bail, Result};
use patcher_core::pe::{
    update_executable_resources, PeVersionInfo, DEFAULT_AS_INVOKER_MANIFEST,
};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        bail!("Usage: stamp_template_pe <exe_path> [ico_path]");
    }

    let exe_path = Path::new(&args[1]);
    let ico_path = args.get(2).map(Path::new);

    let filename = exe_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("patcher_base.exe");
    let mut version_info = PeVersionInfo::default();
    version_info.original_filename = filename.to_string();
    version_info.internal_name = exe_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("patcher_base")
        .to_string();

    update_executable_resources(
        exe_path,
        ico_path,
        Some(&version_info),
        Some(DEFAULT_AS_INVOKER_MANIFEST),
    )?;

    println!(
        "Successfully stamped PE VersionInfo + asInvoker Manifest{} onto {:?}",
        if ico_path.is_some() { " + Icon" } else { "" },
        exe_path
    );
    Ok(())
}
