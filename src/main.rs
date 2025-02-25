#[macro_use]
extern crate rocket;

use rocket::form::{Form, FromForm};
use rocket::fs::{FileServer, NamedFile, TempFile, relative};
use rocket::serde::json::Json;
use std::path::{Path, PathBuf};
use std::time::Instant;
use zip_diff::{FileDifference, compare_zip_files};

mod zip_diff;

#[derive(FromForm)]
struct UploadForm<'r> {
    file1: TempFile<'r>,
    file2: TempFile<'r>,
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

#[get("/<path..>")]
async fn serve(mut path: PathBuf) -> Option<NamedFile> {
    path.set_extension("html");
    let mut path = Path::new(relative!("assets")).join(path);
    if path.is_dir() {
        path.push("index.html");
    }

    NamedFile::open(path).await.ok()
}

// Local mode
#[cfg(not(feature = "shuttle"))]
#[launch]
fn rocket() -> _ {
    rocket::build().mount("/", routes![serve, upload])
}

// Shuttle deployment mode
#[cfg(feature = "shuttle")]
#[shuttle_runtime::main]
async fn main() -> shuttle_rocket::ShuttleRocket {
    let rocket = rocket::build().mount("/", rocket::routes![serve, upload]);

    Ok(rocket.into())
}
