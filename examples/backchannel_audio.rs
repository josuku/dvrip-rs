// use dvrip_rs::{Authentication, Backchannel, Connection, DVRIPCam};
// use std::time::Duration;

// // #[tokio::main]
// async fn main() -> Result<(), Box<dyn std::error::Error>> {
//     // let args: Vec<String> = std::env::args().collect();
//     // if args.len() < 4 {
//     //     println!("Usage: {} <IP> <Username> <Password>", args[0]);
//     //     return Ok(());
//     // }

//     // let ip = &args[1];
//     // let user = &args[2];
//     // let pass = &args[3];

//     // let mut cam = DVRIPCam::new(ip);

//     // println!("Connecting to camera at {}...", ip);
//     // cam.connect(Duration::from_secs(5)).await?;

//     // println!("Logging in as {}...", user);
//     // if !cam.login(user, pass).await? {
//     //     println!("Login failed");
//     //     return Ok(());
//     // }

//     // println!("Starting backchannel (Two-way Audio)...");
//     // cam.start_talk(dvrip_rs::AudioCodec::PCMA).await?;

//     // println!("Sending silence for 1 second to prime buffer...");
//     // let silence = vec![0xD5u8; 320];
//     // for _ in 0..25 {
//     //     // 25 * 40ms = 1000ms
//     //     cam.send_audio(silence.clone()).await?;
//     //     tokio::time::sleep(Duration::from_millis(40)).await;
//     // }

//     // println!("Sending dummy PCMA audio data for 5 seconds...");
//     // // let data = include_bytes!("../output.alaw");

//     // // Send audio in 320-byte chunks (40ms at 8kHz)
//     // // We send slightly faster (35ms) to prevent buffer underrun due to network/processing jitter
//     // for frame in data.chunks(320) {
//     //     cam.send_audio(frame.to_vec()).await?;
//     //     tokio::time::sleep(Duration::from_millis(35)).await;
//     // }
//     // tokio::time::sleep(Duration::from_secs(2)).await;

//     // println!("Stopping backchannel...");
//     // cam.stop_talk().await?;

//     // println!("Closing connection...");
//     // cam.close().await?;

//     // Ok(())
// }

pub fn main() {
    // TODO: fix this example
}
