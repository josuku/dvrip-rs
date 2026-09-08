use dvrip_rs::{Authentication, Connection, DVRIPCam, UserManagement};
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

    println!("--- USER LIST ---");
    match cam.get_users().await {
        Ok(users) => {
            for user in users {
                let name = user
                    .get("Name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("Unknown");
                let group = user.get("Group").and_then(|g| g.as_str()).unwrap_or("None");
                println!(" - User: {} [Group: {}]", name, group);
            }
        }
        Err(e) => eprintln!("Failed to get users: {}", e),
    }

    println!("\n--- GROUP LIST ---");
    match cam.get_groups().await {
        Ok(groups) => {
            for group in groups {
                let name = group
                    .get("Name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("Unknown");
                println!(" - Group: {}", name);
            }
        }
        Err(e) => eprintln!("Failed to get groups: {}", e),
    }

    cam.close().await?;
    Ok(())
}
