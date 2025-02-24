use diff::{Result as DiffResult, lines};
use rayon::prelude::*;
use rocket::serde::{Deserialize, Serialize};
use rustc_hash::FxHasher;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::hash::Hasher;
use std::io::{BufReader, Read};
use std::sync::Mutex;
use std::time::Instant;
use zip::read::ZipArchive;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
pub struct FileDifference {
    filename: String,
    status: String, // "Added", "Removed", "Modified", "Unchanged"
    content_diff: Option<Vec<String>>,
}

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff", "svg"];

/// Compute a fast hash for a ZIP file using FxHasher
fn hash_zip_file(zip_path: &str) -> Result<String, String> {
    let start = Instant::now();

    let file = File::open(zip_path).map_err(|e| format!("Failed to open ZIP: {}", e))?;
    let mut reader = BufReader::new(file);
    let mut hasher = FxHasher::default();
    let mut buffer = [0; 8192];

    while let Ok(n) = reader.read(&mut buffer) {
        if n == 0 {
            break;
        }
        hasher.write(&buffer[..n]);
    }

    println!("✅ ZIP hash computed in {:.2?} seconds", start.elapsed());
    Ok(format!("{:x}", hasher.finish())) // Convert 64-bit hash to hex
}

/// Compute SHA-256 hash of a file's content
fn hash_file_content(content: &[u8]) -> String {
    let mut hasher = FxHasher::default();
    hasher.write(content);
    format!("{:x}", hasher.finish()) // Convert 64-bit hash to hex
}

/// Extract filenames and compute file hashes from a ZIP
fn extract_filenames_and_hashes(zip_path: &str) -> Result<HashMap<String, String>, String> {
    let file = File::open(zip_path).map_err(|_| format!("Failed to open {}", zip_path))?;
    let mut archive = ZipArchive::new(file).map_err(|_| format!("Failed to read {}", zip_path))?;

    let mut files = HashMap::new();

    for i in 0..archive.len() {
        let mut zip_file = archive
            .by_index(i)
            .map_err(|_| "Failed to read file from ZIP")?;
        let name = zip_file.name().to_string();

        if name.starts_with("__MACOSX/") || name.ends_with(".DS_Store") {
            continue;
        }

        let mut buffer = Vec::new();
        zip_file
            .read_to_end(&mut buffer)
            .map_err(|_| format!("Failed to read {}", name))?;
        files.insert(name, hash_file_content(&buffer));
    }

    Ok(files)
}

/// Compare two ZIP files and detect file changes efficiently
pub fn compare_zip_files(zip1: &str, zip2: &str) -> Result<Vec<FileDifference>, String> {
    let start_total = Instant::now();

    // Step 1: Compare ZIP-level hashes
    let hash1 = hash_zip_file(zip1)?;
    let hash2 = hash_zip_file(zip2)?;

    if hash1 == hash2 {
        println!("✅ ZIPs are identical, skipping diff.");
        return Ok(vec![]);
    }

    // Step 2: Extract filenames and compute file hashes
    let files1 = extract_filenames_and_hashes(zip1)?;
    let files2 = extract_filenames_and_hashes(zip2)?;

    let files1_keys: HashSet<String> = files1.keys().cloned().collect();
    let files2_keys: HashSet<String> = files2.keys().cloned().collect();

    let all_files: HashSet<_> = files1_keys.union(&files2_keys).cloned().collect();
    let errors = Mutex::new(Vec::new()); // Thread-safe error collection

    let differences: Vec<FileDifference> = all_files
        .par_iter()
        .filter_map(|filename| {
            let status = match (files1.contains_key(filename), files2.contains_key(filename)) {
                (true, false) => "Removed".to_string(),
                (false, true) => "Added".to_string(),
                (true, true) => {
                    if files1[filename] == files2[filename] {
                        return None;
                    }

                    if IMAGE_EXTENSIONS.iter().any(|ext| filename.ends_with(ext)) {
                        println!("🖼️ Skipping image file: {}", filename);
                        return Some(FileDifference {
                            filename: filename.clone(),
                            status: "Modified (Image)".to_string(),
                            content_diff: None,
                        });
                    }

                    match (
                        extract_file_content(zip1, filename),
                        extract_file_content(zip2, filename),
                    ) {
                        (Ok(content1), Ok(content2)) => {
                            // Detect binary files
                            if content1.as_bytes().contains(&0u8)
                                || content2.as_bytes().contains(&0u8)
                            {
                                return Some(FileDifference {
                                    filename: filename.clone(),
                                    status: "Modified (Binary)".to_string(),
                                    content_diff: None,
                                });
                            }

                            let diff_lines: Vec<String> = lines(&content1, &content2)
                                .into_iter()
                                .filter_map(|diff| match diff {
                                    DiffResult::Left(l) => Some(format!("- {}", l)),
                                    DiffResult::Right(r) => Some(format!("+ {}", r)),
                                    DiffResult::Both(_, _) => None,
                                })
                                .collect();

                            return Some(FileDifference {
                                filename: filename.clone(),
                                status: "Modified".to_string(),
                                content_diff: Some(diff_lines),
                            });
                        }
                        (Err(e1), Err(e2)) => {
                            let mut errs = errors.lock().unwrap();
                            errs.push(format!(
                                "Error comparing file '{}': zip1 error: {:?}, zip2 error: {:?}",
                                filename, e1, e2
                            ));
                        }
                        (Err(e1), _) | (_, Err(e1)) => {
                            let mut errs = errors.lock().unwrap();
                            errs.push(format!("Error extracting file '{}': {:?}", filename, e1));
                        }
                    }
                    return None;
                }
                _ => "Unknown".to_string(),
            };

            Some(FileDifference {
                filename: filename.clone(),
                status,
                content_diff: None,
            })
        })
        .collect();

    println!(
        "⏳ Total comparison time: {:.2?} seconds",
        start_total.elapsed()
    );

    let error_log = errors.into_inner().unwrap();
    if !error_log.is_empty() {
        eprintln!("Encountered errors:\n{}", error_log.join("\n"));
    }

    Ok(differences)
}

/// Extract file content from a ZIP
fn extract_file_content(zip_path: &str, filename: &str) -> Result<String, String> {
    let file = File::open(zip_path).map_err(|_| format!("Failed to open {}", zip_path))?;
    let mut archive = ZipArchive::new(file).map_err(|_| format!("Failed to read {}", zip_path))?;

    for i in 0..archive.len() {
        let mut zip_file = archive
            .by_index(i)
            .map_err(|_| "Failed to read file from ZIP")?;
        if zip_file.name() == filename {
            let mut buffer = Vec::new();
            zip_file
                .read_to_end(&mut buffer)
                .map_err(|_| "Failed to read content".to_string())?;
            return String::from_utf8(buffer).map_err(|_| "Failed to convert to UTF-8".to_string());
        }
    }

    Err(format!("File {} not found in {}", filename, zip_path))
}
