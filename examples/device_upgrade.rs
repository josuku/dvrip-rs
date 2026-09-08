use dvrip_rs::{Authentication, Connection, DVRIPCam, Upgrade};
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

    println!("Checking device upgrade compatibility...");
    match cam.get_upgrade_info().await {
        Ok(info) => println!("Upgrade Information: {:#?}", info),
        Err(e) => eprintln!("Failed to get upgrade info: {}", e),
    }

    println!("\n--- FIRMWARE UPGRADE EXAMPLE ---");
    println!("WARNING: This is a critical operation. Ensure you have the correct .bin file.");

    cam.close().await?;
    Ok(())
}
