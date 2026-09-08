use dvrip_rs::{Authentication, Connection, DVRIPCam, PTZ, PTZCommand};
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

    println!("Performing PTZ operations...");

    // 1. Step movement
    println!("Stepping Left...");
    cam.ptz_step(PTZCommand::DirectionLeft, 10).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    println!("Stepping Right...");
    cam.ptz_step(PTZCommand::DirectionRight, 10).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 2. Continuous movement
    println!("Starting continuous Up movement...");
    cam.ptz(PTZCommand::DirectionUp, 3, -1, 0).await?;

    tokio::time::sleep(Duration::from_secs(1)).await;

    // 3. Zooming (using ptz_step which handles start/stop)
    println!("Zooming Tile...");
    cam.ptz_step(PTZCommand::ZoomTile, 4).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    println!("Zooming Wide...");
    cam.ptz_step(PTZCommand::ZoomWide, 4).await?;

    // 4. Presets
    println!("Moving to Preset 1...");
    cam.ptz(PTZCommand::GotoPreset, 3, 1, 0).await?;

    cam.close().await?;
    println!("Done.");

    Ok(())
}
