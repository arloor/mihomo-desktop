use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const APP_ICON_GROUP_ID: u16 = 1;

fn env_non_empty(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn env_bool(name: &str) -> Option<bool> {
    let raw = env_non_empty(name)?;
    match raw.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn git_success(args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn detect_git_dirty() -> bool {
    git_output(&["status", "--porcelain", "--untracked-files=no"]).is_some()
}

fn head_has_exact_tag(expected_tag: &str) -> bool {
    git_output(&["tag", "--points-at", "HEAD"])
        .map(|tags| tags.lines().any(|line| line.trim() == expected_tag))
        .unwrap_or(false)
}

fn commits_since_ancestor_tag(tag: &str) -> Option<u64> {
    if !git_success(&["merge-base", "--is-ancestor", tag, "HEAD"]) {
        return None;
    }
    let range = format!("{tag}..HEAD");
    git_output(&["rev-list", "--count", &range])?.parse::<u64>().ok()
}

fn emit_git_rerun_hints() {
    let Some(git_dir_raw) = git_output(&["rev-parse", "--absolute-git-dir"]) else {
        return;
    };
    let git_dir = PathBuf::from(git_dir_raw);
    let common_dir = git_output(&["rev-parse", "--git-common-dir"])
        .map(|raw| {
            let path = PathBuf::from(raw);
            if path.is_absolute() {
                path
            } else {
                env::current_dir().map(|cwd| cwd.join(&path)).unwrap_or(path)
            }
        })
        .unwrap_or_else(|| git_dir.clone());

    let head_path = git_dir.join("HEAD");
    emit_rerun_if_exists(&head_path);
    emit_rerun_if_exists(&git_dir.join("index"));
    emit_rerun_if_exists(&common_dir.join("packed-refs"));
    emit_rerun_if_exists(&common_dir.join("refs").join("tags"));

    if let Ok(head_contents) = fs::read_to_string(&head_path) {
        if let Some(reference) = head_contents.trim().strip_prefix("ref: ") {
            emit_rerun_if_exists(&common_dir.join(reference));
        }
    }
}

fn emit_rerun_if_exists(path: &Path) {
    if path.exists() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

fn format_app_version(
    pkg_version: &str,
    git_sha: Option<&str>,
    exact_release_tag: bool,
    commits_since_tag: Option<u64>,
    is_dirty: bool,
) -> String {
    if exact_release_tag {
        if is_dirty {
            return match git_sha {
                Some(sha) => format!("{pkg_version}+dirty.g{sha}"),
                None => format!("{pkg_version}+dirty"),
            };
        }
        return pkg_version.to_string();
    }

    match git_sha {
        Some(sha) => {
            let mut metadata = match commits_since_tag {
                Some(count) if count > 0 => format!("dev.{count}.g{sha}"),
                _ => format!("dev.g{sha}"),
            };
            if is_dirty {
                metadata.push_str(".dirty");
            }
            format!("{pkg_version}+{metadata}")
        }
        None => {
            if is_dirty {
                format!("{pkg_version}+dirty")
            } else {
                pkg_version.to_string()
            }
        }
    }
}

fn collect_enabled_features() -> String {
    let mut features = env::vars()
        .filter_map(|(key, _)| {
            key.strip_prefix("CARGO_FEATURE_")
                .map(str::to_ascii_lowercase)
        })
        .filter(|name| name != "default")
        .collect::<Vec<_>>();
    features.sort();
    if features.is_empty() {
        "none".to_string()
    } else {
        features.join(",")
    }
}

fn configure_version() {
    println!("cargo:rerun-if-env-changed=APP_RELEASE_TAG");
    println!("cargo:rerun-if-env-changed=APP_GIT_SHA");
    println!("cargo:rerun-if-env-changed=APP_GIT_DIRTY");

    emit_git_rerun_hints();

    let pkg_version = env::var("CARGO_PKG_VERSION").expect("Cargo package version");
    let expected_tag = format!("v{pkg_version}");

    let git_sha =
        env_non_empty("APP_GIT_SHA").or_else(|| git_output(&["rev-parse", "--short=8", "HEAD"]));
    let is_dirty = env_bool("APP_GIT_DIRTY").unwrap_or_else(detect_git_dirty);
    let exact_release_tag = match env_non_empty("APP_RELEASE_TAG") {
        Some(tag) => tag == expected_tag,
        None => head_has_exact_tag(&expected_tag),
    };

    let commits_since_tag = if exact_release_tag {
        None
    } else {
        commits_since_ancestor_tag(&expected_tag)
    };

    let app_version = format_app_version(
        &pkg_version,
        git_sha.as_deref(),
        exact_release_tag,
        commits_since_tag,
        is_dirty,
    );
    let features = collect_enabled_features();
    let build_time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %:z");

    println!("cargo:rustc-env=APP_VERSION={app_version}");
    println!(
        "cargo:rustc-env=APP_GIT_SHA={}",
        git_sha.as_deref().unwrap_or("unknown")
    );
    println!("cargo:rustc-env=APP_BUILD_FEATURES={features}");
    println!("cargo:rustc-env=APP_BUILD_TIME={build_time}");
}

fn main() {
    configure_version();
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
