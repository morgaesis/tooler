use crate::platform::get_system_info;
use anyhow::{anyhow, Result};
use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tar::Archive;
use walkdir::WalkDir;

pub async fn download_file(url: &str, local_path: &Path) -> Result<()> {
    tracing::info!("Downloading {}...", local_path.file_name().unwrap().to_string_lossy());
    
    let response = reqwest::get(url).await?;
    let total_size = response.content_length().unwrap_or(0);
    
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-")
    );

    let mut file = fs::File::create(local_path)?;
    let mut downloaded = 0u64;
    let mut stream = response.bytes_stream();

    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }

    pb.finish_with_message("Download complete");
    Ok(())
}

pub fn extract_archive(archive_path: &Path, extract_dir: &Path, tool_name: &str) -> Result<PathBuf> {
    tracing::info!("Extracting {}...", archive_path.file_name().unwrap().to_string_lossy());
    
    let system_info = get_system_info();
    
    if archive_path.extension().and_then(|s| s.to_str()) == Some("zip") {
        extract_zip(archive_path, extract_dir)?;
    } else if archive_path.to_string_lossy().ends_with(".tar.gz") || 
              archive_path.to_string_lossy().ends_with(".tgz") {
        extract_tar_gz(archive_path, extract_dir)?;
    } else if archive_path.to_string_lossy().ends_with(".tar.xz") {
        extract_tar_xz(archive_path, extract_dir)?;
    } else {
        return Err(anyhow!("Unsupported archive format: {}", archive_path.display()));
    }

    let executable_path = find_executable_in_extracted(extract_dir, tool_name, &system_info.os)
        .ok_or_else(|| anyhow!("Could not find executable for {} in extracted archive", tool_name))?;

    // Make executable on Unix-like systems
    if system_info.os != "windows" {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&executable_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&executable_path, perms)?;
        }
    }

    tracing::info!("Successfully extracted and found executable: {}", executable_path.display());
    Ok(executable_path)
}

fn extract_zip(archive_path: &Path, extract_dir: &Path) -> Result<()> {
    let file = fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = extract_dir.join(file.name());
        
        // Security check for path traversal
        if !outpath.starts_with(extract_dir) {
            tracing::warn!("Skipping malicious path in zip: {}", file.name());
            continue;
        }
        
        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = fs::File::create(&outpath)?;
            io::copy(&mut file, &mut outfile)?;
        }
    }
    
    Ok(())
}

fn extract_tar_gz(archive_path: &Path, extract_dir: &Path) -> Result<()> {
    let file = fs::File::open(archive_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    
    for entry in archive.entries()? {
        let mut entry = entry?;
        let outpath = extract_dir.join(entry.path()?);
        
        // Security check for path traversal
        if !outpath.starts_with(extract_dir) {
            tracing::warn!("Skipping malicious path in tar: {:?}", entry.path()?);
            continue;
        }
        
        entry.unpack(&outpath)?;
    }
    
    Ok(())
}

fn extract_tar_xz(archive_path: &Path, extract_dir: &Path) -> Result<()> {
    let file = fs::File::open(archive_path)?;
    let decoder = xz2::read::XzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    
    for entry in archive.entries()? {
        let mut entry = entry?;
        let outpath = extract_dir.join(entry.path()?);
        
        // Security check for path traversal
        if !outpath.starts_with(extract_dir) {
            tracing::warn!("Skipping malicious path in tar: {:?}", entry.path()?);
            continue;
        }
        
        entry.unpack(&outpath)?;
    }
    
    Ok(())
}

fn find_executable_in_extracted(extract_dir: &Path, tool_name: &str, os_system: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    let tool_name_lower = tool_name.to_lowercase();
    
    let target_names = if os_system == "windows" {
        vec![
            format!("{}.exe", tool_name_lower),
            format!("{}.cmd", tool_name_lower),
            format!("{}.bat", tool_name_lower),
            tool_name_lower.clone(),
        ]
    } else {
        vec![
            tool_name_lower.clone(),
            format!("{}.sh", tool_name_lower),
        ]
    };

    for entry in WalkDir::new(extract_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() && is_executable(path, os_system) {
            let file_name = path.file_name()?.to_string_lossy().to_lowercase();
            let file_stem = path.file_stem()?.to_string_lossy().to_lowercase();
            
            let mut score = 0i32;
            
            // Higher score for exact name match
            if target_names.contains(&file_name) {
                score += 100;
            } else if target_names.contains(&file_stem) {
                score += 90;
            } else if file_name.contains(&tool_name_lower) {
                score += 50;
            }
            
            // Penalize deeper paths
            let depth = path.strip_prefix(extract_dir).ok().map_or(0, |p| p.components().count());
            score -= (depth as i32) * 10;
            
            if score > 0 {
                candidates.push((score, path.to_path_buf()));
            }
        }
    }
    
    candidates.sort_by_key(|(score, _)| -(*score));
    candidates.into_iter().map(|(_, path)| path).next()
}

fn is_executable(filepath: &Path, os_system: &str) -> bool {
    if !filepath.is_file() {
        return false;
    }
    
    if os_system == "windows" {
        let ext = filepath.extension().and_then(|s| s.to_str()).unwrap_or("");
        matches!(ext.to_lowercase().as_str(), "exe" | "cmd" | "bat")
    } else {
        // On Unix-like systems, check if it's a regular file and not a library
        let ext = filepath.extension().and_then(|s| s.to_str()).unwrap_or("");
        !matches!(ext.to_lowercase().as_str(), "dll" | "so" | "dylib")
    }
}