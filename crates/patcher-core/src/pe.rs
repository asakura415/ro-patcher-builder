#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use anyhow::{bail, Context, Result};

#[cfg(windows)]
use winapi::shared::ntdef::HANDLE;
#[cfg(windows)]
use winapi::um::winbase::{BeginUpdateResourceW, EndUpdateResourceW, UpdateResourceW};
#[cfg(windows)]
use winapi::um::winuser::{RT_GROUP_ICON, RT_ICON};

#[cfg(windows)]
const RT_VERSION_ID: *const u16 = 16usize as *const u16;
#[cfg(windows)]
const RT_MANIFEST_ID: *const u16 = 24usize as *const u16;
#[cfg(windows)]
const LANG_EN_US: u16 = 0x0409;

const MAX_ICO_IMAGES: usize = 64;

/// Metadata embedded into Windows PE `RT_VERSION` (`VS_VERSIONINFO`) resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeVersionInfo {
    pub company_name: String,
    pub file_description: String,
    pub product_name: String,
    pub file_version: [u16; 4],
    pub product_version: [u16; 4],
    pub original_filename: String,
    pub internal_name: String,
    pub legal_copyright: String,
}

impl Default for PeVersionInfo {
    fn default() -> Self {
        Self {
            company_name: "Modern RO Patcher".to_string(),
            file_description: "Modern RO Client Patcher".to_string(),
            product_name: "Modern RO Patcher".to_string(),
            file_version: [1, 0, 0, 0],
            product_version: [1, 0, 0, 0],
            original_filename: "patcher-template.exe".to_string(),
            internal_name: "patcher-template".to_string(),
            legal_copyright: "Modern RO Patcher Contributors".to_string(),
        }
    }
}

impl PeVersionInfo {
    /// Derives clean, control-character-free PE version metadata from the patcher window title
    /// and output executable filename.
    pub fn from_title_and_filename(title: &str, filename: &str) -> Self {
        let clean = |s: &str, fallback: &str| -> String {
            let filtered: String = s.chars().filter(|c| !c.is_control()).collect();
            let trimmed = filtered.trim();
            if trimmed.is_empty() {
                fallback.to_string()
            } else {
                trimmed.to_string()
            }
        };

        let clean_title = clean(title, "Modern RO Patcher");
        let clean_filename = clean(filename, "Patcher.exe");
        let stem = Path::new(&clean_filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("Patcher")
            .to_string();

        Self {
            company_name: "Modern RO Patcher".to_string(),
            file_description: clean_title.clone(),
            product_name: clean_title,
            file_version: [1, 0, 0, 0],
            product_version: [1, 0, 0, 0],
            original_filename: clean_filename,
            internal_name: stem,
            legal_copyright: "Modern RO Patcher Contributors".to_string(),
        }
    }
}

/// Standard UTF-8 Windows Application Manifest with `asInvoker` execution level,
/// supported OS compatibility GUIDs (Windows 7 through Windows 11), PerMonitorV2 DPI awareness,
/// and long-path support.
pub const DEFAULT_AS_INVOKER_MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity version="1.0.0.0" processorArchitecture="*" name="ModernRO.Patcher" type="win32"/>
  <description>Modern RO Client Patcher</description>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
      <supportedOS Id="{1f676c76-80e1-4239-95bb-83d0f6d0da78}"/>
      <supportedOS Id="{4a2f28e3-53b9-4441-ba9c-d69d4a4a6e38}"/>
      <supportedOS Id="{35138b9a-5d96-4fbd-8e2d-a2440225f93a}"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2, PerMonitor</dpiAwareness>
      <longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>
    </windowsSettings>
  </application>
</assembly>
"#;

fn push_u16_le(buf: &mut Vec<u8>, val: u16) {
    buf.extend_from_slice(&val.to_le_bytes());
}

fn push_u32_le(buf: &mut Vec<u8>, val: u32) {
    buf.extend_from_slice(&val.to_le_bytes());
}

fn push_wide_null(buf: &mut Vec<u8>, s: &str) -> usize {
    let mut count = 0usize;
    for unit in s.encode_utf16().chain(std::iter::once(0u16)) {
        push_u16_le(buf, unit);
        count += 1;
    }
    count
}

fn align4(buf: &mut Vec<u8>) {
    while buf.len() % 4 != 0 {
        buf.push(0);
    }
}

fn patch_block_len(buf: &mut [u8], start: usize) {
    let len = buf.len() - start;
    let len_u16 = u16::try_from(len).unwrap_or(u16::MAX);
    buf[start..start + 2].copy_from_slice(&len_u16.to_le_bytes());
}

fn write_string_entry(buf: &mut Vec<u8>, key: &str, value: &str) {
    let start = buf.len();
    let val_utf16_len = value.encode_utf16().count() + 1; // WORD count including NUL terminator
    push_u16_le(buf, 0); // wLength placeholder
    push_u16_le(buf, u16::try_from(val_utf16_len).unwrap_or(u16::MAX)); // wValueLength (in u16 WORDs)
    push_u16_le(buf, 1); // wType = 1 (text)
    let _ = push_wide_null(buf, key);
    align4(buf);
    let _ = push_wide_null(buf, value);
    patch_block_len(buf, start);
}

fn format_dotted_version(v: [u16; 4]) -> String {
    format!("{}.{}.{}.{}", v[0], v[1], v[2], v[3])
}

/// Serializes a Win32 `VS_VERSIONINFO` resource (`RT_VERSION`, ID 1) according to Microsoft's
/// binary `VS_VERSIONINFO` / `StringFileInfo` / `VarFileInfo` specification.
pub fn build_version_info_resource(info: &PeVersionInfo) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1024);

    // Root VS_VERSIONINFO
    let root_start = buf.len();
    push_u16_le(&mut buf, 0); // wLength placeholder
    push_u16_le(&mut buf, 52); // wValueLength = sizeof(VS_FIXEDFILEINFO) = 52 bytes
    push_u16_le(&mut buf, 0); // wType = 0 (binary)
    let _ = push_wide_null(&mut buf, "VS_VERSION_INFO");
    align4(&mut buf);

    // 52-byte VS_FIXEDFILEINFO
    let fv = info.file_version;
    let pv = info.product_version;
    push_u32_le(&mut buf, 0xFEEF_04BD); // dwSignature
    push_u32_le(&mut buf, 0x0001_0000); // dwStrucVersion (1.0)
    push_u32_le(&mut buf, ((fv[0] as u32) << 16) | (fv[1] as u32)); // dwFileVersionMS
    push_u32_le(&mut buf, ((fv[2] as u32) << 16) | (fv[3] as u32)); // dwFileVersionLS
    push_u32_le(&mut buf, ((pv[0] as u32) << 16) | (pv[1] as u32)); // dwProductVersionMS
    push_u32_le(&mut buf, ((pv[2] as u32) << 16) | (pv[3] as u32)); // dwProductVersionLS
    push_u32_le(&mut buf, 0x0000_003F); // dwFileFlagsMask (VS_FFI_FILEFLAGSMASK)
    push_u32_le(&mut buf, 0x0000_0000); // dwFileFlags
    push_u32_le(&mut buf, 0x0004_0004); // dwFileOS (VOS_NT_WINDOWS32)
    push_u32_le(&mut buf, 0x0000_0001); // dwFileType (VFT_APP)
    push_u32_le(&mut buf, 0x0000_0000); // dwFileSubtype
    push_u32_le(&mut buf, 0x0000_0000); // dwFileDateMS
    push_u32_le(&mut buf, 0x0000_0000); // dwFileDateLS
    align4(&mut buf);

    // Child 1: StringFileInfo
    let sfi_start = buf.len();
    push_u16_le(&mut buf, 0); // wLength placeholder
    push_u16_le(&mut buf, 0); // wValueLength = 0
    push_u16_le(&mut buf, 1); // wType = 1 (text)
    let _ = push_wide_null(&mut buf, "StringFileInfo");
    align4(&mut buf);

    // Child of StringFileInfo: StringTable ("040904B0" = US English + Unicode UTF-16)
    let st_start = buf.len();
    push_u16_le(&mut buf, 0); // wLength placeholder
    push_u16_le(&mut buf, 0); // wValueLength = 0
    push_u16_le(&mut buf, 1); // wType = 1 (text)
    let _ = push_wide_null(&mut buf, "040904B0");
    align4(&mut buf);

    let file_ver_str = format_dotted_version(info.file_version);
    let prod_ver_str = format_dotted_version(info.product_version);
    let entries = [
        ("CompanyName", info.company_name.as_str()),
        ("FileDescription", info.file_description.as_str()),
        ("FileVersion", file_ver_str.as_str()),
        ("InternalName", info.internal_name.as_str()),
        ("LegalCopyright", info.legal_copyright.as_str()),
        ("OriginalFilename", info.original_filename.as_str()),
        ("ProductName", info.product_name.as_str()),
        ("ProductVersion", prod_ver_str.as_str()),
    ];

    for (idx, (k, v)) in entries.iter().enumerate() {
        if idx > 0 {
            align4(&mut buf);
        }
        write_string_entry(&mut buf, k, v);
    }

    patch_block_len(&mut buf, st_start);
    patch_block_len(&mut buf, sfi_start);

    // Align before sibling VarFileInfo
    align4(&mut buf);

    // Child 2: VarFileInfo
    let vfi_start = buf.len();
    push_u16_le(&mut buf, 0); // wLength placeholder
    push_u16_le(&mut buf, 0); // wValueLength = 0
    push_u16_le(&mut buf, 1); // wType = 1 (text)
    let _ = push_wide_null(&mut buf, "VarFileInfo");
    align4(&mut buf);

    // Child of VarFileInfo: Var ("Translation")
    let var_start = buf.len();
    push_u16_le(&mut buf, 0); // wLength placeholder
    push_u16_le(&mut buf, 4); // wValueLength = 4 bytes (u16 lang + u16 codepage)
    push_u16_le(&mut buf, 0); // wType = 0 (binary)
    let _ = push_wide_null(&mut buf, "Translation");
    align4(&mut buf);
    push_u16_le(&mut buf, 0x0409); // US English
    push_u16_le(&mut buf, 0x04B0); // Unicode (1200)
    patch_block_len(&mut buf, var_start);

    patch_block_len(&mut buf, vfi_start);
    patch_block_len(&mut buf, root_start);
    buf
}

fn parse_ico_for_pe(ico_bytes: &[u8]) -> Result<(Vec<u8>, Vec<(u16, Vec<u8>)>)> {
    if ico_bytes.len() < 6 {
        bail!("Invalid icon file: header too short.");
    }

    let id_type = u16::from_le_bytes([ico_bytes[2], ico_bytes[3]]);
    let id_count = u16::from_le_bytes([ico_bytes[4], ico_bytes[5]]) as usize;

    if id_type != 1 || id_count == 0 {
        bail!("Invalid icon file: not a valid .ico format or contains no images.");
    }
    if id_count > MAX_ICO_IMAGES {
        bail!(
            "Invalid icon file: image count ({}) exceeds maximum supported ({}).",
            id_count,
            MAX_ICO_IMAGES
        );
    }

    let mut group_icon_header = Vec::with_capacity(6 + id_count * 14);
    group_icon_header.extend_from_slice(&ico_bytes[0..6]);

    let mut icon_images: Vec<(u16, Vec<u8>)> = Vec::with_capacity(id_count);

    for i in 0..id_count {
        let entry_offset = 6usize
            .checked_add(i * 16)
            .ok_or_else(|| anyhow::anyhow!("Icon entry offset overflow"))?;
        let entry_end = entry_offset
            .checked_add(16)
            .ok_or_else(|| anyhow::anyhow!("Icon entry end overflow"))?;
        if entry_end > ico_bytes.len() {
            bail!("Truncated icon directory entries.");
        }

        let entry_slice = &ico_bytes[entry_offset..entry_end];
        let bytes_in_res = u32::from_le_bytes(entry_slice[8..12].try_into()?);
        let image_offset = u32::from_le_bytes(entry_slice[12..16].try_into()?);

        let img_start = image_offset as usize;
        let img_end = img_start
            .checked_add(bytes_in_res as usize)
            .ok_or_else(|| anyhow::anyhow!("Icon image size overflow"))?;

        if img_end > ico_bytes.len() {
            bail!("Truncated icon image data.");
        }

        let image_data = ico_bytes[img_start..img_end].to_vec();
        if image_data.len() > u32::MAX as usize {
            bail!("Icon image exceeds u32 resource size limit.");
        }
        let icon_id = (i + 1) as u16;

        group_icon_header.extend_from_slice(&entry_slice[0..12]);
        group_icon_header.extend_from_slice(&icon_id.to_le_bytes());

        icon_images.push((icon_id, image_data));
    }

    if group_icon_header.len() > u32::MAX as usize {
        bail!("Icon group header exceeds u32 resource size limit.");
    }

    Ok((group_icon_header, icon_images))
}

#[cfg(windows)]
struct ResourceUpdateGuard {
    handle: HANDLE,
    committed: bool,
}

#[cfg(windows)]
impl ResourceUpdateGuard {
    fn begin(wide_path: &[u16]) -> Result<Self> {
        let handle = unsafe { BeginUpdateResourceW(wide_path.as_ptr(), 0) };
        if handle.is_null() {
            bail!("BeginUpdateResourceW failed");
        }
        Ok(Self {
            handle,
            committed: false,
        })
    }

    fn update(&self, res_type: *const u16, res_id: *const u16, lang: u16, data: &[u8]) -> Result<()> {
        let len = u32::try_from(data.len())
            .map_err(|_| anyhow::anyhow!("PE resource size exceeds u32::MAX"))?;
        let res = unsafe {
            UpdateResourceW(
                self.handle,
                res_type,
                res_id,
                lang,
                data.as_ptr() as *mut _,
                len,
            )
        };
        if res == 0 {
            bail!("UpdateResourceW failed");
        }
        Ok(())
    }

    fn commit(mut self) -> Result<()> {
        let ok = unsafe { EndUpdateResourceW(self.handle, 0) };
        self.committed = true;
        if ok == 0 {
            bail!("EndUpdateResourceW failed committing resource updates.");
        }
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for ResourceUpdateGuard {
    fn drop(&mut self) {
        if !self.committed && !self.handle.is_null() {
            unsafe {
                let _ = EndUpdateResourceW(self.handle, 1);
            }
        }
    }
}

/// Updates PE resources (icon, `RT_VERSION`, and `RT_MANIFEST`) in a Windows executable file.
/// NOTE: As specified in PROJECT_PLAN.md, this must always be performed BEFORE appending binary overlay.
pub fn update_executable_resources(
    exe_path: impl AsRef<Path>,
    ico_path: Option<&Path>,
    version_info: Option<&PeVersionInfo>,
    manifest_xml: Option<&str>,
) -> Result<()> {
    let exe = exe_path.as_ref();
    if !exe.exists() {
        bail!("Executable file not found at {:?}", exe);
    }

    let parsed_ico = if let Some(ico) = ico_path {
        if !ico.exists() {
            bail!("Icon file not found at {:?}", ico);
        }
        let ico_bytes =
            std::fs::read(ico).with_context(|| format!("Failed to read icon file at {:?}", ico))?;
        Some(parse_ico_for_pe(&ico_bytes)?)
    } else {
        None
    };

    let version_bytes = version_info.map(build_version_info_resource);
    let manifest_bytes = manifest_xml.map(|s| s.as_bytes());

    if parsed_ico.is_none() && version_bytes.is_none() && manifest_bytes.is_none() {
        return Ok(());
    }

    #[cfg(windows)]
    {
        let wide_path: Vec<u16> = OsStr::new(exe)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let guard = ResourceUpdateGuard::begin(&wide_path)
            .with_context(|| format!("BeginUpdateResourceW failed for executable {:?}", exe))?;

        if let Some((ref group_icon_header, ref icon_images)) = parsed_ico {
            for (id, img_data) in icon_images {
                guard
                    .update(RT_ICON, *id as usize as *const u16, 0, img_data)
                    .with_context(|| format!("UpdateResourceW failed for RT_ICON id {}", id))?;
            }
            guard
                .update(RT_GROUP_ICON, 1usize as *const u16, 0, group_icon_header)
                .with_context(|| "UpdateResourceW failed for RT_GROUP_ICON")?;
        }

        if let Some(ref ver_data) = version_bytes {
            guard
                .update(RT_VERSION_ID, 1usize as *const u16, LANG_EN_US, ver_data)
                .with_context(|| "UpdateResourceW failed for RT_VERSION")?;
        }

        if let Some(man_data) = manifest_bytes {
            guard
                .update(RT_MANIFEST_ID, 1usize as *const u16, LANG_EN_US, man_data)
                .with_context(|| "UpdateResourceW failed for RT_MANIFEST")?;
        }

        guard.commit()?;
    }

    #[cfg(not(windows))]
    {
        let _ = (parsed_ico, version_bytes, manifest_bytes);
        log::warn!("PE Resource updating is only supported on Windows.");
    }

    log::info!("Successfully updated PE resources in {:?}", exe);
    Ok(())
}

/// Updates the main application icon (.ico) in a Windows PE executable file.
/// NOTE: As specified in PROJECT_PLAN.md, this must always be performed BEFORE appending binary overlay.
pub fn update_executable_icon(exe_path: impl AsRef<Path>, ico_path: impl AsRef<Path>) -> Result<()> {
    update_executable_resources(exe_path, Some(ico_path.as_ref()), None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_version_info_resource_structure() {
        let info = PeVersionInfo::from_title_and_filename(
            "PRONTERA BOX - Patcher\n\r",
            "_PRONTERA BOX - Patcher.exe",
        );
        assert_eq!(info.file_description, "PRONTERA BOX - Patcher");
        assert_eq!(info.original_filename, "_PRONTERA BOX - Patcher.exe");
        assert_eq!(info.internal_name, "_PRONTERA BOX - Patcher");

        let bytes = build_version_info_resource(&info);
        assert!(bytes.len() >= 92);

        // Root wLength matches exact byte length
        let root_len = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
        assert_eq!(root_len, bytes.len());
        // wValueLength = 52 bytes for VS_FIXEDFILEINFO
        let val_len = u16::from_le_bytes([bytes[2], bytes[3]]);
        assert_eq!(val_len, 52);
        // wType = 0 (binary)
        let w_type = u16::from_le_bytes([bytes[4], bytes[5]]);
        assert_eq!(w_type, 0);

        // VS_FIXEDFILEINFO signature (0xFEEF04BD) must be 4-byte aligned right after "VS_VERSION_INFO\0"
        let sig_bytes = 0xFEEF_04BDu32.to_le_bytes();
        let sig_pos = bytes
            .windows(4)
            .position(|w| w == sig_bytes)
            .expect("VS_FIXEDFILEINFO signature not found");
        assert_eq!(sig_pos % 4, 0);

        // Verify StringFileInfo & StringTable keys and CompanyName UTF-16LE presence
        let company_u16: Vec<u8> = "CompanyName"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        assert!(bytes.windows(company_u16.len()).any(|w| w == company_u16.as_slice()));

        let translation_u16: Vec<u8> = "Translation"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        assert!(bytes
            .windows(translation_u16.len())
            .any(|w| w == translation_u16.as_slice()));
    }

    #[test]
    fn test_ico_parser_rejects_overflow_and_truncated() {
        // Too short
        assert!(parse_ico_for_pe(&[0, 0, 1, 0]).is_err());

        // id_count = 65 exceeds MAX_ICO_IMAGES (64)
        let too_many = [0, 0, 1, 0, 65, 0];
        assert!(parse_ico_for_pe(&too_many).is_err());

        // Image offset + bytes_in_res integer overflow
        let mut overflow = vec![0u8; 22];
        overflow[2..4].copy_from_slice(&1u16.to_le_bytes()); // ICON type
        overflow[4..6].copy_from_slice(&1u16.to_le_bytes()); // 1 image
        overflow[14..18].copy_from_slice(&u32::MAX.to_le_bytes()); // bytes_in_res
        overflow[18..22].copy_from_slice(&u32::MAX.to_le_bytes()); // image_offset
        assert!(parse_ico_for_pe(&overflow).is_err());

        // Truncated data
        let mut trunc = vec![0u8; 22];
        trunc[2..4].copy_from_slice(&1u16.to_le_bytes());
        trunc[4..6].copy_from_slice(&1u16.to_le_bytes());
        trunc[14..18].copy_from_slice(&100u32.to_le_bytes());
        trunc[18..22].copy_from_slice(&22u32.to_le_bytes());
        assert!(parse_ico_for_pe(&trunc).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn test_update_executable_resources_preserves_overlay() {
        use crate::config::PatcherConfig;
        use crate::overlay::{
            append_overlay_to_file, create_overlay_zip, parse_embedded_assets,
            read_overlay_from_reader,
        };
        use std::fs::File;

        let dir = tempfile::tempdir().unwrap();
        let exe_copy = dir.path().join("stamped_test.exe");
        let base_src = std::env::current_exe().unwrap();
        std::fs::copy(&base_src, &exe_copy).unwrap();

        let ico_candidate = Path::new("../../template-dist/default_icon.ico");
        let ico_opt = if ico_candidate.exists() {
            Some(ico_candidate)
        } else {
            None
        };

        let ver = PeVersionInfo::from_title_and_filename("Audit Test Patcher", "stamped_test.exe");
        update_executable_resources(&exe_copy, ico_opt, Some(&ver), Some(DEFAULT_AS_INVOKER_MANIFEST))
            .unwrap();

        // Append ROPATCH1 overlay after PE resource update
        let theme_dir = tempfile::tempdir().unwrap();
        std::fs::write(theme_dir.path().join("index.html"), b"<h1>PE Ok</h1>").unwrap();
        let mut cfg = PatcherConfig::default();
        cfg.window.title = "Audit Test Patcher".to_string();
        let payload = create_overlay_zip(&cfg, theme_dir.path()).unwrap();
        append_overlay_to_file(&exe_copy, &payload).unwrap();

        let mut f = File::open(&exe_copy).unwrap();
        let read_back = read_overlay_from_reader(&mut f).unwrap().unwrap();
        let assets = parse_embedded_assets(&read_back).unwrap();
        assert_eq!(assets.config.window.title, "Audit Test Patcher");
        assert_eq!(assets.files["index.html"].0, b"<h1>PE Ok</h1>");
    }
}
