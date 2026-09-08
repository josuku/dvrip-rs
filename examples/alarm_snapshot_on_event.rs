use dvrip_rs::{Alarm, Authentication, Connection, DVRIPCam, Monitoring};
use std::time::Duration;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

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

    let (tx, mut rx) = tokio::sync::mpsc::channel(32);

    println!("Setting up automated capture on motion...");

    let callback = Box::new(move |data: serde_json::Value, count| {
        println!(
            "Alarm received (count {}). Signal sent to capture logic.",
            count
        );
        if let Err(e) = tx.try_send(data) {
            eprintln!("Failed to send event to processor: {}", e);
        }
    });

    cam.set_alarm_callback(Some(callback));
    cam.start_alarm_monitoring().await?;

    println!("Monitoring... Will save snapshots to the current directory.");

    while let Some(_event_data) = rx.recv().await {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("snapshot_{}.jpg", timestamp);

        println!("Event detected! Capturing image {}...", filename);

        match cam.snapshot(0).await {
            Ok(image_bytes) => {
                let mut file = File::create(&filename).await?;
                file.write_all(&image_bytes).await?;
                println!(
                    "  Successfully saved {} ({} bytes)",
                    filename,
                    image_bytes.len()
                );
            }
            Err(e) => eprintln!("  Failed to take snapshot: {}", e),
        }
    }

    cam.close().await?;
    Ok(())
}
