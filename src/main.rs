#[macro_use]
extern crate rocket;

use rocket::form::{Form, FromForm};
use rocket::fs::{NamedFile, TempFile};
use rocket::serde::json::Json;

use std::path::Path;
use std::time::Instant;

mod zip_diff;
use zip_diff::{FileDifference, compare_zip_files};

#[derive(FromForm)]
struct UploadForm<'r> {
    file1: TempFile<'r>,
    file2: TempFile<'r>, // Now mandatory
}

#[post("/upload", data = "<form>")]
async fn upload(
    mut form: Form<UploadForm<'_>>,
) -> Result<Json<Vec<FileDifference>>, rocket::http::Status> {
    let start_time = Instant::now();

    // Save files temporarily
    let temp1 = "tmp/temp1.zip";
    let temp2 = "tmp/temp2.zip";

    if let Err(e) = form.file1.persist_to(temp1).await {
        eprintln!("❌ Failed to save file1: {}", e);
        return Err(rocket::http::Status::InternalServerError);
    }

    if let Err(e) = form.file2.persist_to(temp2).await {
        eprintln!("❌ Failed to save file2: {}", e);
        return Err(rocket::http::Status::InternalServerError);
    }

    // Perform file diff
    let result = compare_zip_files(temp1, temp2);

    // Cleanup temp files
    let _ = std::fs::remove_file(temp1);
    let _ = std::fs::remove_file(temp2);

    match result {
        Ok(diff) => {
            println!(
                "✅ compare_zip_files execution time: {:?}",
                start_time.elapsed()
            );
            Ok(Json(diff))
        }
        Err(e) => {
            eprintln!(
                "❌ Error in compare_zip_files: {:?} (Execution time: {:?})",
                e,
                start_time.elapsed()
            );
            Err(rocket::http::Status::BadRequest)
        }
    }
}

#[get("/diff")]
async fn file_diff() -> Result<Json<Vec<FileDifference>>, rocket::http::Status> {
    let zip1 = "exports/zipOriginal1.zip";
    let zip2 = "exports/test.zip";

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
