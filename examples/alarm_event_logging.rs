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

    println!("Connecting and logging in...");
    cam.connect(Duration::from_secs(5)).await?;

    if !cam.login(user, pass).await? {
        println!("Login failed");
        return Ok(());
    }

    println!("Starting alarm monitoring...");

    let callback = Box::new(|data: serde_json::Value, count| {
        let now = chrono::Local::now();
        println!("\n[{}] EVENT #{}", now.format("%H:%M:%S"), count);

        if let Some(obj) = data.as_object() {
            for (key, value) in obj {
                println!("  {}: {}", key, value);
            }
        } else {
            println!("  Raw Data: {}", data);
        }
    });

    cam.set_alarm_callback(Some(callback));
    cam.start_alarm_monitoring().await?;

    println!("Monitoring for 2 minutes. Press Ctrl+C to stop early.");
    tokio::time::sleep(Duration::from_secs(120)).await;

    println!("Stopping monitoring...");
    cam.stop_alarm_monitoring().await?;
    cam.close().await?;

    Ok(())
}
