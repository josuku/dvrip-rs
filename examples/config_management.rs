use dvrip_rs::{Authentication, Connection, DVRIPCam, SystemInfo};
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

    println!("--- CONFIGURATION MANAGEMENT ---");

    // 1. Get Encoding Configuration
    println!("Retrieving Encoding Config...");
    match cam.get_encode_info(false).await {
        Ok(config) => println!("Current Encode Settings: {:#?}", config),
        Err(e) => eprintln!("Error: {}", e),
    }

    // 2. Get Camera Settings
    println!("\nRetrieving Camera Settings...");
    match cam.get_camera_info(false).await {
        Ok(config) => println!("Current Camera Settings: {:#?}", config),
        Err(e) => eprintln!("Error: {}", e),
    }

    // 3. Syncing device time
    println!("\nSyncing device time with local system time...");
    let now = chrono::Local::now();
    match cam.set_time(Some(now)).await {
        Ok(true) => println!("Time synchronized to: {}", now),
        Ok(false) => println!("Failed to sync time."),
        Err(e) => eprintln!("Error: {}", e),
    }

    cam.close().await?;
    Ok(())
}
