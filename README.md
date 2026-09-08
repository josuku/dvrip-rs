# dvrip-rs

A high-performance Rust library for the **DVRIP** protocol.

This library is a port of the original Python implementation, optimized for concurrency

Each feature is separated as an trait so you can use only the features you need.

This crate was made for use with another program i am still making but i decided to put it in this repository

## Features

- [x] **Authentication**: Secure login and session management.
- [x] **Real-time Monitoring**: Stream live video (H.264/H.265) directly from the device.
- [x] **Video Recording**: Save streams to local storage.
- [x] **System Information**: Retrieve device hardware and software details.
- [x] **User Management**: Manage accounts and permissions. ( WIP )
- [x] **PTZ Control**: Remote Pan, Tilt, and Zoom operations.
- [x] **Alarm Monitoring**: Asynchronous callback system for alarm events. ( WIP )
- [x] **File Management**: List and search for recordings on the device.
- [x] **Upgrade**: Upgrade the device firmware. ( WIP )
- [ ] **Backchannel**: Two-way audio communication. (it works but lags for some reason, i will investigate)

## Quick Start

### Installation

Add `dvrip` to your `Cargo.toml`:

```toml
[dependencies]
dvrip = { git = "https://github.com/orpos/dvrip_rs" }
tokio = { version = "1.0", features = ["full"] }
```

### Basic Example

Connect to a camera and start monitoring:

```rust
use dvrip::DVRIPCam;
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize and Connect
    let mut cam = DVRIPCam::new("192.168.0.100");
    cam.connect(tokio::time::Duration::from_secs(10)).await?;

    // 2. Login
    if !cam.login("admin", "password").await? {
        panic!("Login failed");
    }

    // 3. Start Video Monitor
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    cam.start_monitor(
        Box::new(move |frame, metadata| {
            // Filter for I-frames or P-frames
            if metadata.frame_type.is_some() {
                tx.send(frame).unwrap();
            }
        }),
        "Main",
        0,
    ).await?;

    // 4. Handle incoming frames 
    // ( the output is a raw stream of h265 data and not a containerized file you can use tools like ffmpeg to convert it to a containerized file this also applies to downloaded recordings )
    let mut file = tokio::fs::File::create("output.h265").await?;
    while let Some(data) = rx.recv().await {
        file.write_all(&data).await?;
    }

    Ok(())
}
```

## Credits & References

- https://github.com/OpenIPC/python-dvr
- https://github.com/AlexxIT/go2rtc
- https://github.com/alexshpilkin/dvrip

## License

MIT
