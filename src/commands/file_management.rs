use crate::error::Result;
use crate::{DVRIPError, dvrip::DVRIPCam};
use async_trait::async_trait;
use chrono::{DateTime, Local};
use serde_json::{Value, json};
use std::path::Path;
use tokio::{fs::File, io::AsyncWriteExt};

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

        while let Some((header, data)) = rx.recv().await {
            if header.data_len == 0 {
                break;
            }
            receiver
                .send(data)
                .await
                .map_err(|_| DVRIPError::Unknown("Failed to send".to_string()))?;
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

        while let Some((header, data)) = rx.recv().await {
            if header.data_len == 0 {
                break;
            }
            file.write_all(&data).await?;
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
}
