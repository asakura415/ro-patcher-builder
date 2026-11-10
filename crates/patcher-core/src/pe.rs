#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use anyhow::{bail, Context, Result};

#[cfg(windows)]
use winapi::um::winbase::{BeginUpdateResourceW, EndUpdateResourceW, UpdateResourceW};
#[cfg(windows)]
use winapi::um::winuser::{RT_GROUP_ICON, RT_ICON};

/// Updates the main application icon (.ico) in a Windows PE executable file.
/// NOTE: As specified in PROJECT_PLAN.md, this must always be performed BEFORE appending binary overlay.
pub fn update_executable_icon(exe_path: impl AsRef<Path>, ico_path: impl AsRef<Path>) -> Result<()> {
    let exe = exe_path.as_ref();
    let ico = ico_path.as_ref();

    if !exe.exists() {
        bail!("Executable file not found at {:?}", exe);
    }
    if !ico.exists() {
        bail!("Icon file not found at {:?}", ico);
    }

    let ico_bytes = std::fs::read(ico)
        .with_context(|| format!("Failed to read icon file at {:?}", ico))?;

    if ico_bytes.len() < 6 {
        bail!("Invalid icon file: header too short.");
    }

    // Parse ICONDIR
    let id_type = u16::from_le_bytes([ico_bytes[2], ico_bytes[3]]);
    let id_count = u16::from_le_bytes([ico_bytes[4], ico_bytes[5]]) as usize;

    if id_type != 1 || id_count == 0 {
        bail!("Invalid icon file: not a valid .ico format or contains no images.");
    }

    let mut group_icon_header = Vec::new();
    // GRPICONDIR: idReserved (2), idType (2), idCount (2)
    group_icon_header.extend_from_slice(&ico_bytes[0..6]);

    let mut icon_images: Vec<(u16, Vec<u8>)> = Vec::new();

    for i in 0..id_count {
        let entry_offset = 6 + i * 16;
        if entry_offset + 16 > ico_bytes.len() {
            bail!("Truncated icon directory entries.");
        }

        let entry_slice = &ico_bytes[entry_offset..entry_offset + 16];
        let bytes_in_res = u32::from_le_bytes(entry_slice[8..12].try_into()?);
        let image_offset = u32::from_le_bytes(entry_slice[12..16].try_into()?);

        let img_start = image_offset as usize;
        let img_end = img_start + bytes_in_res as usize;

        if img_end > ico_bytes.len() {
            bail!("Truncated icon image data.");
        }

        let image_data = ico_bytes[img_start..img_end].to_vec();
        let icon_id = (i + 1) as u16;

        // Build GRPICONDIRENTRY (14 bytes):
        // width, height, colors, reserved, planes, bitcount, bytes_in_res, nID (2 bytes)
        group_icon_header.extend_from_slice(&entry_slice[0..12]);
        group_icon_header.extend_from_slice(&icon_id.to_le_bytes());

        icon_images.push((icon_id, image_data));
    }

    #[cfg(windows)]
    {
        let wide_path: Vec<u16> = OsStr::new(exe)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let h_update = BeginUpdateResourceW(wide_path.as_ptr(), 0);
            if h_update.is_null() {
                bail!("BeginUpdateResourceW failed for executable {:?}", exe);
            }

            // 1. Write individual RT_ICON resources
            for (id, img_data) in &icon_images {
                let res = UpdateResourceW(
                    h_update,
                    RT_ICON,
                    *id as usize as *const u16,
                    0, // LANG_NEUTRAL
                    img_data.as_ptr() as *mut _,
                    img_data.len() as u32,
                );
                if res == 0 {
                    let _ = EndUpdateResourceW(h_update, 1);
                    bail!("UpdateResourceW failed for RT_ICON id {}", id);
                }
            }

            // 2. Write main RT_GROUP_ICON resource (ID = 1, e.g. MAINICON)
            let res = UpdateResourceW(
                h_update,
                RT_GROUP_ICON,
                1 as usize as *const u16,
                0, // LANG_NEUTRAL
                group_icon_header.as_ptr() as *mut _,
                group_icon_header.len() as u32,
            );
            if res == 0 {
                let _ = EndUpdateResourceW(h_update, 1);
                bail!("UpdateResourceW failed for RT_GROUP_ICON");
            }

            let commit = EndUpdateResourceW(h_update, 0);
            if commit == 0 {
                bail!("EndUpdateResourceW failed committing resource updates.");
            }
        }
    }

    #[cfg(not(windows))]
    {
        log::warn!("PE Resource updating is only supported on Windows.");
    }

    log::info!("Successfully updated icon in {:?}", exe);
    Ok(())
}
