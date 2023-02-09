#[macro_use]
extern crate rocket;
extern crate ffmpeg_next as ffmpeg;

pub mod cors;
pub mod utils;

use rocket::data::ToByteUnit;
use rocket::fs::TempFile;
use rocket::http::ContentType;
use rocket::response::stream::ReaderStream;
use rocket::serde::{json::Json, Serialize};
use rocket::tokio::fs::File;
use std::str::FromStr;

use std::env;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cors::CORS;
// use utils;
use rocket::form::{Form, FromForm};

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct Reels {
    size: u64,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct ReelsConfig {
    id: u128,
    number_of_reels: i64,
    reels: Vec<Reels>,
}

#[derive(FromForm)]
struct ReelsForm<'r> {
    // maximum number of second a reels can be
    max_reels_duration: i64,
    // #[field(validate = ext(ContentType::from_str("application/video").unwrap()))]
    file: TempFile<'r>,
}

#[post("/start_reels", format = "multipart/form-data", data = "<form>")]
async fn start_reels(mut form: Form<ReelsForm<'_>>) -> Json<ReelsConfig> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();

    let filepath = format!(
        "{}reels_{}.mp4",
        env::temp_dir().to_str().unwrap(),
        timestamp
    );
    form.file
        .persist_to(&filepath)
        .await
        .expect("Couldn't persist temp file to storage");

    println!("FilePath {}", filepath);

    let duration = utils::get_video_duration(&filepath);
    println!("Duration: {}", duration);

    let number_of_reels = utils::get_number_of_reels(duration, form.max_reels_duration);
    let extra_seconds = duration % form.max_reels_duration;
    println!("Number of reels need: {}", number_of_reels);

    let mut reels: Vec<Reels> = Vec::with_capacity(number_of_reels as usize);

    for index in 0..number_of_reels {
        let output_file = format!(
            "{}reels_{}-{}.mp4",
            env::temp_dir().to_str().unwrap(),
            timestamp,
            index
        );

        let start_time = index * form.max_reels_duration;

        let end_time = {
            if index + 1 < number_of_reels {
                form.max_reels_duration * (index + 1)
            } else {
                (form.max_reels_duration * (index + 1)) + extra_seconds
            }
        };

        let mut command = Command::new("ffmpeg");
        command.args([
            "-i",
            &filepath,
            "-ss",
            &utils::convert_timestamp(start_time),
            "-to",
            &utils::convert_timestamp(end_time),
            "-c",
            "copy",
            &output_file,
        ]);
        println!("{:?}", command.get_args().collect::<Vec<&OsStr>>());

        let output = command
            .status()
            .expect("Video processing failed on ffmpeg command");
        assert!(
            output.success(),
            "Video processing failed on ffmpeg command"
        );
        // in bytes
        let file_size = std::fs::File::open(&output_file)
            .unwrap()
            .metadata()
            .unwrap()
            .len();
        reels.push(Reels { size: file_size })
    }

    Json(ReelsConfig {
        id: timestamp,
        number_of_reels: number_of_reels,
        reels: reels,
    })
}

#[derive(FromForm)]
struct ReelsQuery {
    id: u128,
    start_index: u8,
    max_reels_duration: i64,
}

#[get("/get_reels?<query..>")]
fn get_reels(query: ReelsQuery) -> ReaderStream![File] {
    let filepath = format!(
        "{}reels_{}.mp4",
        env::temp_dir().to_str().unwrap(),
        query.id
    );

    //// check if file exist
    assert!(Path::new(&filepath).exists(), "File Doesn't exists");

    let duration = utils::get_video_duration(&filepath);

    println!("Duration: {}", duration);
    let number_of_reels = utils::get_number_of_reels(duration, query.max_reels_duration);

    ReaderStream! {
        for index in (query.start_index as i64)..number_of_reels {
            let output_file = format!("{}reels_{}-{}.mp4", env::temp_dir().to_str().unwrap(), query.id, index);

            if let Ok(file) = File::open(output_file).await {
                yield file;
            }
        }
    }
}

#[launch]
fn rocket() -> _ {
    println!("{}", 100.megabytes().as_u128());
    // println!("{}", ContentType:: );
    rocket::build()
        .attach(CORS)
        .mount("/", routes![start_reels, get_reels])
}
