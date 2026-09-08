use crate::constants::OK_CODES;
use crate::dvrip::DVRIPCam;
use crate::error::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

pub type UpgradeProgressCallback = Box<dyn Fn(String) + Send + Sync>;

#[async_trait]
pub trait Upgrade: Send + Sync {
    /// Get upgrade information
    async fn get_upgrade_info(&self) -> Result<Value>;

    /// Perform system upgrade
    async fn upgrade(
        &self,
        filename: &str,
        packet_size: usize,
        progress_callback: Option<UpgradeProgressCallback>,
    ) -> Result<Value>;
}

#[async_trait]
impl Upgrade for DVRIPCam {
    async fn get_upgrade_info(&self) -> Result<Value> {
        self.get_command("OPSystemUpgrade", None).await
    }

    async fn upgrade(
        &self,
        filename: &str,
        packet_size: usize,
        progress_callback: Option<UpgradeProgressCallback>,
    ) -> Result<Value> {
        // Iniciar upgrade
        let start_data = json!({
            "Action": "Start",
            "Type": "System",
        });

        let reply = self
            .set_command("OPSystemUpgrade", start_data, Some(0x5F0))
            .await?;

        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64())
            && !OK_CODES.contains(&(ret as u32))
        {
            return Ok(reply);
        }

        let callback = progress_callback.map(Arc::new);

        // Send file
        let mut file = File::open(filename).await?;
        let mut blocknum = 0u32;
        let file_metadata = file.metadata().await?;
        let file_size = file_metadata.len() as usize;
        let mut sent_bytes = 0usize;

        let pool = self.send_pool.as_ref().clone().ok_or_else(|| {
            crate::error::DVRIPError::ConnectionError("Did you connect to the camera?".to_string())
        })?;

        let session = self.session_id();
        let upgrade_msg_id = 0x5F2;

        loop {
            let mut buffer = vec![0u8; packet_size];
            let bytes_read = file.read(&mut buffer).await?;

            if bytes_read == 0 {
                break;
            }

            buffer.truncate(bytes_read);
            buffer.extend_from_slice(b"\x0a\x00"); // Append tail for version 0

            let header = crate::protocol::PacketHeader {
                data_len: buffer.len() as u32,
                msg_id: upgrade_msg_id,
                packet_count: blocknum,
                session,
                head: 0xFF,
                version: 0,
            };

            let (send, recv) =
                tokio::sync::oneshot::channel::<(crate::protocol::PacketHeader, Vec<u8>)>();

            let request = crate::dvrip::CommandRequest::new(header, buffer)
                .with_response(send)
                .with_counter(false)
                .with_expected_response(upgrade_msg_id);

            pool.send(request).await.map_err(|_| {
                crate::error::DVRIPError::ConnectionError(
                    "Failed to send upgrade packet".to_string(),
                )
            })?;

            // Wait for partial ACK
            let (reply_header, reply_data_raw) = recv.await.map_err(|_| {
                crate::error::DVRIPError::ConnectionError(
                    "Failed to receive upgrade response".to_string(),
                )
            })?;

            if reply_header.msg_id == upgrade_msg_id {
                let reply_data =
                    serde_json::from_slice::<Value>(&reply_data_raw[..reply_data_raw.len() - 2])
                        .map_err(|_| {
                            crate::error::DVRIPError::SerializationError(
                                "Failed to parse upgrade response".to_string(),
                            )
                        })?;

                if let Some(ret) = reply_data.get("Ret").and_then(|r| r.as_u64())
                    && ret != 100
                {
                    if let Some(cb) = &callback {
                        cb("Upgrade failed".to_string());
                    }
                    return Ok(reply_data);
                }
            }

            blocknum += 1;
            sent_bytes += bytes_read;

            // Progress
            if let Some(cb) = &callback {
                let progress = (sent_bytes as f64 / file_size as f64) * 100.0;
                cb(format!("Uploading: {:.1}%", progress));
            }
        }

        let mut final_packet = vec![0u8; 0];
        final_packet.extend_from_slice(b"\x0a\x00");
        let header = crate::protocol::PacketHeader {
            data_len: final_packet.len() as u32,
            msg_id: upgrade_msg_id,
            packet_count: blocknum,
            session,
            head: 0xFF,
            version: 0,
        };
        let (send, recv) =
            tokio::sync::oneshot::channel::<(crate::protocol::PacketHeader, Vec<u8>)>();

        let request = crate::dvrip::CommandRequest::new(header, final_packet)
            .with_response(send)
            .with_counter(false)
            .with_expected_response(upgrade_msg_id);

        pool.send(request).await.map_err(|_| {
            crate::error::DVRIPError::ConnectionError(
                "Failed to send final upgrade packet".to_string(),
            )
        })?;

        let _ = recv.await; // Consume the immediate ACK for the empty packet

        // Wait for upgrade start confirmation (persistent listener)
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        self.stream_handlers.insert(upgrade_msg_id, tx);

        let result = async {
            loop {
                // Wait for packets with 0x5F2
                if let Some((_, reply_data_raw)) = rx.recv().await {
                    let reply_data = match serde_json::from_slice::<Value>(
                        &reply_data_raw[..reply_data_raw.len() - 2],
                    ) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    if let Some(ret) = reply_data.get("Ret").and_then(|r| r.as_u64()) {
                        if ret == 515 {
                            if let Some(cb) = &callback {
                                cb("Upgrade successful".to_string());
                            }
                            return Ok(reply_data);
                        } else if [512, 513, 514].contains(&(ret as u32)) {
                            if let Some(cb) = &callback {
                                cb("Upgrade failed".to_string());
                            }
                            return Ok(reply_data);
                        } else if ret <= 100
                            && let Some(cb) = &callback
                        {
                            cb(format!("Upgrading: {}%", ret));
                        }
                    }
                } else {
                    return Err(crate::error::DVRIPError::ConnectionError(
                        "Stream closed unexpectedly".to_string(),
                    ));
                }
            }
        }
        .await;

        self.stream_handlers.remove(&upgrade_msg_id);
        result
    }
}
