use crate::error::Result;
use crate::protocol::PacketHeader;
use crate::{DVRIPError, dvrip::DVRIPCam};
use async_trait::async_trait;
use chrono::{DateTime, Local};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::Duration;
use tokio::{fs::File, io::AsyncWriteExt};

async fn recv_stream_packet(
    rx: &mut tokio::sync::mpsc::Receiver<(PacketHeader, Vec<u8>)>,
    connected: &Arc<AtomicBool>,
) -> Option<(PacketHeader, Vec<u8>)> {
    loop {
        tokio::select! {
            biased;
            msg = rx.recv() => return msg,
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                if !connected.load(Ordering::Acquire) {
                    return None;
                }
                // connected. waiting for data
            }
        }
    }
}

#[async_trait]
pub trait FileManagement: Send + Sync {
    /// List local files on the device
    async fn list_local_files(
        &self,
        start_time: DateTime<Local>,
        end_time: DateTime<Local>,
        file_type: &str,
        channel: u8,
    ) -> Result<Vec<Value>>;

    /// Download a file from the device
    async fn download_file(
        &self,
        start_time: DateTime<Local>,
        end_time: DateTime<Local>,
        filename: &str,
        target_path: &str,
    ) -> Result<()>;

    /// Streams a file from the device
    async fn stream_file(
        &self,
        start_time: DateTime<Local>,
        end_time: DateTime<Local>,
        filename: &str,
        receiver: tokio::sync::mpsc::Sender<Vec<u8>>,
    ) -> Result<()>;

    /// Download by a real time range instead of a specific filename.
    async fn download_file_by_time(
        &self,
        start_time: DateTime<Local>,
        end_time: DateTime<Local>,
        channel: u8,
        target_path: &str,
    ) -> Result<()>;

    /// Streaming counterpart of `download_file_by_time`
    async fn stream_file_by_time(
        &self,
        start_time: DateTime<Local>,
        end_time: DateTime<Local>,
        channel: u8,
        receiver: tokio::sync::mpsc::Sender<Vec<u8>>,
    ) -> Result<()>;
}

#[async_trait]
impl FileManagement for DVRIPCam {
    async fn list_local_files(
        &self,
        start_time: DateTime<Local>,
        end_time: DateTime<Local>,
        file_type: &str,
        channel: u8,
    ) -> Result<Vec<Value>> {
        let start_str = start_time.format("%Y-%m-%d %H:%M:%S").to_string();
        let end_str = end_time.format("%Y-%m-%d %H:%M:%S").to_string();

        let data = json!({
            "Name": "OPFileQuery",
            "OPFileQuery": {
                "BeginTime": start_str,
                "Channel": channel,
                "DriverTypeMask": "0x0000FFFF",
                "EndTime": end_str,
                "Event": "*",
                "StreamType": "0x00000000",
                "Type": file_type,
            },
        });

        let mut reply = self
            .send_command(1440, data, true)
            .await?
            .ok_or_else(|| crate::error::DVRIPError::ProtocolError("Empty response".to_string()))?;

        let mut result = Vec::new();

        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64())
            && ret != 100
        {
            return Ok(vec![]);
        }

        if let Some(files) = reply.get_mut("OPFileQuery").and_then(|f| f.as_array()) {
            result.extend_from_slice(files);
        }

        // OPFileQuery only returns the first 64 items
        // We need to keep querying until we get all
        while let Some(files) = reply.get("OPFileQuery").and_then(|f| f.as_array()) {
            if files.len() != 64 {
                break;
            };

            let Some(last_file) = files.last() else {
                break;
            };

            let Some(new_start) = last_file.get("BeginTime").and_then(|t| t.as_str()) else {
                break;
            };

            let data = json!({
                "Name": "OPFileQuery",
                "OPFileQuery": {
                    "BeginTime": new_start,
                    "Channel": channel,
                    "DriverTypeMask": "0x0000FFFF",
                    "EndTime": end_str,
                    "Event": "*",
                    "StreamType": "0x00000000",
                    "Type": file_type,
                },
            });

            reply = self.send_command(1440, data, true).await?.ok_or_else(|| {
                crate::error::DVRIPError::ProtocolError("Resposta vazia".to_string())
            })?;

            let Some(new_files) = reply.get("OPFileQuery").and_then(|f| f.as_array()) else {
                break;
            };

            if new_files.is_empty() {
                break;
            }
            result.extend(new_files.clone());
        }

        Ok(result)
    }

    async fn stream_file(
        &self,
        start_time: DateTime<Local>,
        end_time: DateTime<Local>,
        filename: &str,
        receiver: tokio::sync::mpsc::Sender<Vec<u8>>,
    ) -> Result<()> {
        let start_str = start_time.format("%Y-%m-%d %H:%M:%S").to_string();
        let end_str = end_time.format("%Y-%m-%d %H:%M:%S").to_string();

        // Claim
        let claim_data = json!({
            "Name": "OPPlayBack",
            "OPPlayBack": {
                "Action": "Claim",
                "Parameter": {
                    "PlayMode": "ByName",
                    "FileName": filename,
                    "StreamType": 0,
                    "Value": 0,
                    "TransMode": "TCP",
                },
                "StartTime": start_str,
                "EndTime": end_str,
            },
        });

        self.send_command(1424, claim_data, true).await?;

        // Prepare stream listener
        let (tx, mut rx) = tokio::sync::mpsc::channel(100);
        let stream_ids = [0x1FC, 0x1FD, 0x1FA, 0x1F9, 0x5FC, 0x0592]; // Standard media + explicit stream ID
        for &id in &stream_ids {
            self.stream_handlers.insert(id, tx.clone());
        }

        // DownloadStart
        let download_start_data = json!({
            "Name": "OPPlayBack",
            "OPPlayBack": {
                "Action": "DownloadStart",
                "Parameter": {
                    "PlayMode": "ByName",
                    "FileName": filename,
                    "StreamType": 0,
                    "Value": 0,
                    "TransMode": "TCP",
                },
                "StartTime": start_str,
                "EndTime": end_str,
            },
        });

        self.send_command(1420, download_start_data, false).await?;

        let mut received_any = false;
        while let Some((header, data)) = recv_stream_packet(&mut rx, &self.connected).await {
            if header.data_len == 0 {
                break; // explicit stream end
            }
            received_any = true;
            receiver
                .send(data)
                .await
                .map_err(|_| DVRIPError::Unknown("Failed to send".to_string()))?;
        }
        if !received_any {
            return Err(DVRIPError::ConnectionError(
                "no data received before connection was closed".to_string(),
            ));
        }

        // Cleanup handlers
        for &id in &stream_ids {
            self.stream_handlers.remove(&id);
        }

        // DownloadStop
        let download_stop_data = json!({
            "Name": "OPPlayBack",
            "OPPlayBack": {
                "Action": "DownloadStop",
                "Parameter": {
                    "FileName": filename,
                    "PlayMode": "ByName",
                    "StreamType": 0,
                    "TransMode": "TCP",
                    "Channel": 0,
                    "Value": 0,
                },
                "StartTime": start_str,
                "EndTime": end_str,
            },
        });

        self.send_command(1420, download_stop_data, false).await?;

        Ok(())
    }

    // TODO: migrate this to use stream_file
    async fn download_file(
        &self,
        start_time: DateTime<Local>,
        end_time: DateTime<Local>,
        filename: &str,
        target_path: &str,
    ) -> Result<()> {
        if let Some(parent) = Path::new(target_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let start_str = start_time.format("%Y-%m-%d %H:%M:%S").to_string();
        let end_str = end_time.format("%Y-%m-%d %H:%M:%S").to_string();

        // Claim
        let claim_data = json!({
            "Name": "OPPlayBack",
            "OPPlayBack": {
                "Action": "Claim",
                "Parameter": {
                    "PlayMode": "ByName",
                    "FileName": filename,
                    "StreamType": 0,
                    "Value": 0,
                    "TransMode": "TCP",
                },
                "StartTime": start_str,
                "EndTime": end_str,
            },
        });

        self.send_command(1424, claim_data, true).await?;

        // Prepare stream listener
        let (tx, mut rx) = tokio::sync::mpsc::channel(100);
        let stream_ids = [0x1FC, 0x1FD, 0x1FA, 0x1F9, 0x5FC, 0x0592]; // Standard media + explicit stream ID
        for &id in &stream_ids {
            self.stream_handlers.insert(id, tx.clone());
        }

        // DownloadStart
        let download_start_data = json!({
            "Name": "OPPlayBack",
            "OPPlayBack": {
                "Action": "DownloadStart",
                "Parameter": {
                    "PlayMode": "ByName",
                    "FileName": filename,
                    "StreamType": 0,
                    "Value": 0,
                    "TransMode": "TCP",
                },
                "StartTime": start_str,
                "EndTime": end_str,
            },
        });

        self.send_command(1420, download_start_data, false).await?;

        // Receive data and write to file
        let mut file = File::create(target_path).await?;

        let mut received_any = false;
        while let Some((header, data)) = recv_stream_packet(&mut rx, &self.connected).await {
            if header.data_len == 0 {
                break; // explicit stream end
            }
            received_any = true;
            file.write_all(&data).await?;
        }

        if !received_any {
            return Err(DVRIPError::ConnectionError(
                "no data received before connection was closed".to_string(),
            ));
        }
        file.sync_all().await?;

        // Cleanup handlers
        for &id in &stream_ids {
            self.stream_handlers.remove(&id);
        }

        // DownloadStop
        let download_stop_data = json!({
            "Name": "OPPlayBack",
            "OPPlayBack": {
                "Action": "DownloadStop",
                "Parameter": {
                    "FileName": filename,
                    "PlayMode": "ByName",
                    "StreamType": 0,
                    "TransMode": "TCP",
                    "Channel": 0,
                    "Value": 0,
                },
                "StartTime": start_str,
                "EndTime": end_str,
            },
        });

        self.send_command(1420, download_stop_data, false).await?;

        Ok(())
    }

    async fn download_file_by_time(
        &self,
        start_time: DateTime<Local>,
        end_time: DateTime<Local>,
        channel: u8,
        target_path: &str,
    ) -> Result<()> {
        if let Some(parent) = Path::new(target_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let start_str = start_time.format("%Y-%m-%d %H:%M:%S").to_string();
        let end_str = end_time.format("%Y-%m-%d %H:%M:%S").to_string();

        // Claim
        let claim_data = json!({
            "Name": "OPPlayBack",
            "OPPlayBack": {
                "Action": "Claim",
                "Parameter": {
                    "PlayMode": "ByTime",
                    "FileName": "",
                    "Channel": channel,
                    "StreamType": 0,
                    "Value": 0,
                    "TransMode": "TCP",
                },
                "StartTime": start_str,
                "EndTime": end_str,
            },
        });

        self.send_command(1424, claim_data, true).await?;

        // Prepare stream listener
        let (tx, mut rx) = tokio::sync::mpsc::channel(100);
        let stream_ids = [0x1FC, 0x1FD, 0x1FA, 0x1F9, 0x5FC, 0x0592]; // Standard media + explicit stream ID
        for &id in &stream_ids {
            self.stream_handlers.insert(id, tx.clone());
        }

        // DownloadStart
        let download_start_data = json!({
            "Name": "OPPlayBack",
            "OPPlayBack": {
                "Action": "DownloadStart",
                "Parameter": {
                    "PlayMode": "ByTime",
                    "FileName": "",
                    "Channel": channel,
                    "StreamType": 0,
                    "Value": 0,
                    "TransMode": "TCP",
                },
                "StartTime": start_str,
                "EndTime": end_str,
            },
        });

        self.send_command(1420, download_start_data, false).await?;

        // Receive data and write to file
        let mut file = File::create(target_path).await?;

        let mut received_any = false;
        while let Some((header, data)) = recv_stream_packet(&mut rx, &self.connected).await {
            if header.data_len == 0 {
                break; // explicit stream end
            }
            received_any = true;
            file.write_all(&data).await?;
        }

        if !received_any {
            return Err(DVRIPError::ConnectionError(
                "no data received before connection was closed".to_string(),
            ));
        }
        file.sync_all().await?;

        // Cleanup handlers
        for &id in &stream_ids {
            self.stream_handlers.remove(&id);
        }

        // DownloadStop
        let download_stop_data = json!({
            "Name": "OPPlayBack",
            "OPPlayBack": {
                "Action": "DownloadStop",
                "Parameter": {
                    "FileName": "",
                    "PlayMode": "ByTime",
                    "StreamType": 0,
                    "TransMode": "TCP",
                    "Channel": channel,
                    "Value": 0,
                },
                "StartTime": start_str,
                "EndTime": end_str,
            },
        });

        self.send_command(1420, download_stop_data, false).await?;

        Ok(())
    }

    async fn stream_file_by_time(
        &self,
        start_time: DateTime<Local>,
        end_time: DateTime<Local>,
        channel: u8,
        receiver: tokio::sync::mpsc::Sender<Vec<u8>>,
    ) -> Result<()> {
        let start_str = start_time.format("%Y-%m-%d %H:%M:%S").to_string();
        let end_str = end_time.format("%Y-%m-%d %H:%M:%S").to_string();

        // Claim
        let claim_data = json!({
            "Name": "OPPlayBack",
            "OPPlayBack": {
                "Action": "Claim",
                "Parameter": {
                    "PlayMode": "ByTime",
                    "FileName": "",
                    "Channel": channel,
                    "StreamType": 0,
                    "Value": 0,
                    "TransMode": "TCP",
                },
                "StartTime": start_str,
                "EndTime": end_str,
            },
        });

        self.send_command(1424, claim_data, true).await?;

        // Prepare stream listener
        let (tx, mut rx) = tokio::sync::mpsc::channel(100);
        let stream_ids = [0x1FC, 0x1FD, 0x1FA, 0x1F9, 0x5FC, 0x0592]; // Standard media + explicit stream ID
        for &id in &stream_ids {
            self.stream_handlers.insert(id, tx.clone());
        }

        // DownloadStart
        let download_start_data = json!({
            "Name": "OPPlayBack",
            "OPPlayBack": {
                "Action": "DownloadStart",
                "Parameter": {
                    "PlayMode": "ByTime",
                    "FileName": "",
                    "Channel": channel,
                    "StreamType": 0,
                    "Value": 0,
                    "TransMode": "TCP",
                },
                "StartTime": start_str,
                "EndTime": end_str,
            },
        });

        self.send_command(1420, download_start_data, false).await?;

        let mut received_any = false;
        while let Some((header, data)) = recv_stream_packet(&mut rx, &self.connected).await {
            if header.data_len == 0 {
                break; // explicit stream end
            }
            received_any = true;
            receiver
                .send(data)
                .await
                .map_err(|_| DVRIPError::Unknown("Failed to send".to_string()))?;
        }
        if !received_any {
            return Err(DVRIPError::ConnectionError(
                "no data received before connection was closed".to_string(),
            ));
        }

        // Cleanup handlers
        for &id in &stream_ids {
            self.stream_handlers.remove(&id);
        }

        // DownloadStop
        let download_stop_data = json!({
            "Name": "OPPlayBack",
            "OPPlayBack": {
                "Action": "DownloadStop",
                "Parameter": {
                    "FileName": "",
                    "PlayMode": "ByTime",
                    "StreamType": 0,
                    "TransMode": "TCP",
                    "Channel": channel,
                    "Value": 0,
                },
                "StartTime": start_str,
                "EndTime": end_str,
            },
        });

        self.send_command(1420, download_stop_data, false).await?;

        Ok(())
    }
}
