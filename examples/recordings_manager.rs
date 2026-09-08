use chrono::{Duration as ChronoDuration, Local};
use dvrip_rs::{Authentication, Connection, DVRIPCam, FileManagement};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        println!("Usage: {} <IP> <Username> <Password>", args[0]);
        return Ok(());
    }

    let ip = &args[1];
    let user = &args[2];
    let pass = &args[3];

    let mut cam = DVRIPCam::new(ip);

    cam.connect(Duration::from_secs(5)).await?;
    cam.login(user, pass).await?;

    println!("Searching for video files from the last 24 hours...");

    let end_time = Local::now();
    let start_time = end_time - ChronoDuration::hours(24);

    match cam.list_local_files(start_time, end_time, "video", 0).await {
        Ok(files) => {
            println!("Found {} files.", files.len());

            for (i, file) in files.iter().take(5).enumerate() {
                let name = file
                    .get("FileName")
                    .and_then(|f| f.as_str())
                    .unwrap_or("Unknown");
                let size_str = file
                    .get("FileLength")
                    .and_then(|s| s.as_str())
                    .unwrap_or("0");
                let size = u64::from_str_radix(size_str.trim_start_matches("0x"), 16).unwrap_or(0);
                let begin = file
                    .get("BeginTime")
                    .and_then(|t| t.as_str())
                    .unwrap_or("?");

                println!(
                    "{}. {} ({:?} MB) - Start: {}",
                    i + 1,
                    name,
                    size as f64 / 1024.0,
                    begin
                );

                if i == 0 {
                    let target = "downloaded_video.h265";
                    println!(
                        "Downloading first file to '{}' (this may take time)...",
                        target
                    );

                    match cam.download_file(start_time, end_time, name, target).await {
                        Ok(_) => println!("Download complete! saved to {}", target),
                        Err(e) => eprintln!("Download failed: {}", e),
                    }
                }
            }
        }
        Err(e) => eprintln!("Error listing files: {}", e),
    }

    cam.close().await?;
    Ok(())
}
