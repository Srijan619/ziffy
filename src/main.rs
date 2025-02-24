#[macro_use]
extern crate rocket;

use rocket::form::{Form, FromForm};
use rocket::fs::{NamedFile, TempFile};
use rocket::serde::{Serialize, json::Json};
use std::path::Path;
use std::time::Instant;

mod zip_diff;
use zip_diff::{FileDifference, compare_zip_files};

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct FileMetadata {
    filename: String,
    content_length: u64,
}

#[derive(FromForm)]
struct UploadForm<'r> {
    file1: TempFile<'r>,
    file2: Option<TempFile<'r>>, // Second file is optional
}

#[post("/upload", data = "<form>")]
async fn upload(
    mut form: Form<UploadForm<'_>>,
) -> Result<Json<Vec<FileMetadata>>, rocket::http::Status> {
    let mut metadata_list = Vec::new();

    // Handle the first file
    let file1 = &mut form.file1;
    let file1_name = file1.name().unwrap_or("unknown").to_string();
    let file1_size = file1.len();
    metadata_list.push(FileMetadata {
        filename: file1_name,
        content_length: file1_size,
    });

    // Handle the second file if it exists
    if let Some(file2) = &mut form.file2 {
        let file2_name = file2.name().unwrap_or("unknown").to_string();
        let file2_size = file2.len();
        metadata_list.push(FileMetadata {
            filename: file2_name,
            content_length: file2_size,
        });
    }

    // Reject request if more than 2 files were uploaded
    if metadata_list.len() > 2 {
        return Err(rocket::http::Status::NotFound);
    }

    Ok(Json(metadata_list))
}

#[get("/diff")]
async fn file_diff() -> Result<Json<Vec<FileDifference>>, rocket::http::Status> {
    let zip1 = "zipOriginal1.zip";
    let zip2 = "test.zip";

    if !Path::new(zip1).exists() || !Path::new(zip2).exists() {
        return Err(rocket::http::Status::NotFound);
    }

    let start_time = Instant::now(); // Start timer

    match compare_zip_files(zip1, zip2) {
        Ok(diff) => {
            let duration = start_time.elapsed(); // Calculate elapsed time
            println!("compare_zip_files execution time: {:?}", duration);
            Ok(Json(diff))
        }
        Err(e) => {
            let duration = start_time.elapsed();
            eprintln!(
                "Error in compare_zip_files: {:?} (Execution time: {:?})",
                e, duration
            );
            Err(rocket::http::Status::BadRequest)
        }
    }
}

#[get("/")]
async fn index() -> Option<NamedFile> {
    NamedFile::open("index.html").await.ok()
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/", routes![upload, index, file_diff])
}
