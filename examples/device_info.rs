use dvrip_rs::{Authentication, Connection, DVRIPCam, SystemInfo};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        println!("Usage: {} <IP> <Username> <Password>", args[0]);
        println!("Example: cargo run --example device_info -- 192.168.1.10 admin pass123");
        return Ok(());
    }

    let ip = &args[1];
    let user = &args[2];
    let pass = &args[3];

    // 1. Initialize the camera client
    let mut cam = DVRIPCam::new(ip);

    // 2. Connect to the device
    println!("Connecting to {}...", ip);
    cam.connect(Duration::from_secs(5)).await?;

    // 3. Login
    println!("Logging in as {}...", user);
    if cam.login(user, pass).await? {
        println!("Login successful!");
    } else {
        println!("Login failed!");
        return Ok(());
    }

    // 4. Retrieve System Information
    println!("\n--- General Info ---");
    match cam.get_general_info().await {
        Ok(general) => println!("{:#?}", general),
        Err(e) => eprintln!("Error getting general info: {}", e),
    }

    println!("\n--- Network Info ---");
    match cam.get_network_info().await {
        Ok(network) => println!("{:#?}", network),
        Err(e) => eprintln!("Error getting network info: {}", e),
    }

    println!("\n--- Device Time ---");
    match cam.get_time().await {
        Ok(dev_time) => println!("Current device time: {}", dev_time),
        Err(e) => eprintln!("Error getting device time: {}", e),
    }

    println!("\n--- System Capabilities ---");
    match cam.get_system_capabilities().await {
        Ok(caps) => println!("{:#?}", caps),
        Err(e) => eprintln!("Error getting system capabilities: {}", e),
    }

    // 5. Close connection
    cam.close().await?;
    println!("\nDisconnected.");

    Ok(())
}
