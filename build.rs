use std::env;
use std::fs;
use std::path::PathBuf;

const APP_ICON_GROUP_ID: u16 = 1;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let ico_path = manifest_dir.join("assets").join("icon.ico");
    let ico_bytes = fs::read(&ico_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", ico_path.display()));

    let res_bytes = build_win32_icon_res(&ico_bytes)
        .unwrap_or_else(|e| panic!("invalid ICO file {}: {e}", ico_path.display()));

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let res_path = out_dir.join("app_icon.res");
    fs::write(&res_path, res_bytes)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", res_path.display()));

    // Link the compiled Win32 icon resource directly into the mihomo-desktop binary
    // so Windows Explorer, Taskbar, Alt+Tab, Start Menu, and LoadImageW(IDI=1) use it.
    println!(
        "cargo:rustc-link-arg-bin=mihomo-desktop={}",
        res_path.display()
    );
}

/// Compile a Windows `.ico` file directly into a standards-compliant 32-bit Win32 `.res`
/// binary resource file containing `RT_ICON` (type 3) entries and an `RT_GROUP_ICON`
/// (type 14, ID 1) directory. No external `rc.exe` required.
fn build_win32_icon_res(ico: &[u8]) -> Result<Vec<u8>, String> {
    if ico.len() < 6 {
        return Err("file smaller than ICONDIR header".into());
    }
    let reserved = u16::from_le_bytes([ico[0], ico[1]]);
    let icon_type = u16::from_le_bytes([ico[2], ico[3]]);
    let count = u16::from_le_bytes([ico[4], ico[5]]);
    if reserved != 0 || icon_type != 1 || count == 0 {
        return Err(format!(
            "unexpected ICONDIR header: reserved={reserved}, type={icon_type}, count={count}"
        ));
    }

    let dir_end = 6usize + (count as usize) * 16;
    if ico.len() < dir_end {
        return Err("truncated ICONDIRENTRY table".into());
    }

    let mut out = Vec::with_capacity(ico.len() + 512);

    // Required 32-byte empty resource header identifying a 32-bit .RES file
    write_resource_entry(&mut out, 0, 0, &[], 0);

    // Build RT_GROUP_ICON (NEWHEADER + RESDIR[]) while emitting each RT_ICON entry
    let mut group_data = Vec::with_capacity(6 + (count as usize) * 14);
    group_data.extend_from_slice(&0u16.to_le_bytes()); // idReserved
    group_data.extend_from_slice(&1u16.to_le_bytes()); // idType = 1 (icon)
    group_data.extend_from_slice(&count.to_le_bytes()); // idCount

    for idx in 0..count {
        let entry_off = 6 + (idx as usize) * 16;
        let entry = &ico[entry_off..entry_off + 16];
        let bytes_in_res = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
        let image_offset =
            u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
        let image_end = image_offset
            .checked_add(bytes_in_res)
            .ok_or("icon entry offset overflow")?;
        if image_end > ico.len() {
            return Err(format!(
                "icon entry {idx} exceeds file size ({image_end} > {})",
                ico.len()
            ));
        }

        let icon_id = idx + 1;
        // RT_ICON = 3
        write_resource_entry(&mut out, 3, icon_id, &ico[image_offset..image_end], 0x0409);

        // GRPICONDIRENTRY: first 12 bytes identical to ICONDIRENTRY + u16 nID
        group_data.extend_from_slice(&entry[0..12]);
        group_data.extend_from_slice(&icon_id.to_le_bytes());
    }

    // RT_GROUP_ICON = 14, ID = APP_ICON_GROUP_ID (1)
    write_resource_entry(&mut out, 14, APP_ICON_GROUP_ID, &group_data, 0x0409);

    Ok(out)
}

fn write_resource_entry(out: &mut Vec<u8>, res_type: u16, res_id: u16, data: &[u8], lang_id: u16) {
    // Fixed 32-byte RESOURCEHEADER when both Type and Name are numeric IDs (0xFFFF, id)
    let data_size = data.len() as u32;
    let header_size: u32 = 32;

    out.extend_from_slice(&data_size.to_le_bytes()); // DataSize
    out.extend_from_slice(&header_size.to_le_bytes()); // HeaderSize
    // TYPE (numeric ID)
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&res_type.to_le_bytes());
    // NAME (numeric ID)
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&res_id.to_le_bytes());
    // DataVersion, MemoryFlags (MOVEABLE | PURE = 0x0030 for non-empty), LanguageId, Version, Characteristics
    out.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    let mem_flags: u16 = if res_type == 0 && res_id == 0 {
        0
    } else {
        0x0030
    };
    out.extend_from_slice(&mem_flags.to_le_bytes()); // MemoryFlags
    out.extend_from_slice(&lang_id.to_le_bytes()); // LanguageId
    out.extend_from_slice(&0u32.to_le_bytes()); // Version
    out.extend_from_slice(&0u32.to_le_bytes()); // Characteristics

    out.extend_from_slice(data);
    // Pad payload to 4-byte DWORD alignment
    while out.len() % 4 != 0 {
        out.push(0);
    }
}
