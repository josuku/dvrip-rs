use dvrip_rs::{Alarm, Authentication, Connection, DVRIPCam};
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

    println!("Attempting to trigger the device's remote alarm/siren output...");

    match cam.set_remote_alarm(true).await {
        Ok(true) => println!("Alarm activated! Check your device."),
        Ok(false) => println!("Command sent but device returned failure."),
        Err(e) => eprintln!("Error sending command: {}", e),
    }

    println!("Waiting 5 seconds before deactivating...");
    tokio::time::sleep(Duration::from_secs(5)).await;

    println!("Deactivating alarm...");
    let _ = cam.set_remote_alarm(false).await;

    cam.close().await?;
    println!("Done.");

    Ok(())
}
