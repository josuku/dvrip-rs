use std::sync::atomic::Ordering;

use crate::constants::QCODES;
use crate::dvrip::DVRIPCam;
use crate::error::Result;
use async_trait::async_trait;
use serde_json::json;

// This is based of the go2rtc implementation

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioCodec {
    PCMA,
    PCMU,
}

#[async_trait]
pub trait Backchannel: Send + Sync {
    /// Start the backchannel (talk) with the device
    async fn start_talk(&self, codec: AudioCodec) -> Result<()>;

    /// Send audio data to the device
    /// Ensure start_talk is called first and successful
    async fn send_audio(&self, data: Vec<u8>) -> Result<()>;

    /// Stop the backchannel
    async fn stop_talk(&self) -> Result<()>;
}

#[async_trait]
impl Backchannel for DVRIPCam {
    async fn start_talk(&self, codec: AudioCodec) -> Result<()> {
        let cmd = "OPTalk";
        let code = QCODES.get(cmd).copied().unwrap_or(1434);

        // Claim the channel
        let data = json!({
            "Action": "Claim",
            "AudioFormat": {
                "EncodeType": match codec {
                    AudioCodec::PCMA => "G711_ALAW",
                    AudioCodec::PCMU => "G711_ULAW",
                },
            }
        });

        // We expect a response to confirm claim
        self.set_command(cmd, data, Some(code as u32)).await?;

        let session = self.session.load(Ordering::Acquire);

        let start = json!({
            "Name" : cmd,
            "SessionID": format!("0x{:08X}", session),
            "OPTalk" : {
                "Action": "Start",
                "AudioFormat": {
                    "EncodeType": match codec {
                        AudioCodec::PCMA => "G711_ALAW",
                        AudioCodec::PCMU => "G711_ULAW",
                    },
                }
            }
        });
        // self.set_command(cmd, start, Some(0x0596)).await?;
        let start_code = QCODES.get("OPTalkStart").copied().unwrap_or(1430);
        self.send_command(start_code, start, false).await?;

        *self.codec.lock().await = Some(codec);

        Ok(())
    }

    async fn send_audio(&self, data: Vec<u8>) -> Result<()> {
        let Some(codec) = *self.codec.lock().await else {
            return Err(crate::DVRIPError::NotInitialized());
        };

        let mut buffer = self.backchannel_buffer.lock().await;
        buffer.extend_from_slice(&data);

        let cmd = "OPTalkData";
        let code = QCODES.get(cmd).copied().unwrap_or(1432);
        let packet_size = 320;

        let codec_id = match codec {
            AudioCodec::PCMA => 14,
            AudioCodec::PCMU => 10,
        };

        while buffer.len() >= packet_size {
            let chunk: Vec<u8> = buffer.drain(0..packet_size).collect();

            let mut buf = Vec::with_capacity(8 + packet_size);
            // Header: 0x000001FA (Big Endian)
            buf.extend_from_slice(&0x1FAu32.to_be_bytes());
            // Byte 4: Codec (14 for PCMA, 10 for PCMU)
            buf.push(codec_id);
            // Byte 5: Sample Rate Index (2 for 8000Hz)
            buf.push(2);
            // Bytes 6-7: Payload Length (Little Endian)
            buf.extend_from_slice(&(packet_size as u16).to_le_bytes());
            // Payload
            buf.extend_from_slice(&chunk);

            self.send_raw_packet(code, buf, false, false).await?;
        }

        Ok(())
    }

    async fn stop_talk(&self) -> Result<()> {
        let cmd = "OPTalk";
        let code = QCODES.get(cmd).copied().unwrap_or(1434);

        let data = json!({
            "Name": cmd,
            "SessionID": format!("0x{:08X}", self.session_id()),
            "OPTalk": {
                "Action": "Stop"
            }
        });

        self.set_command(cmd, data["OPTalk"].clone(), Some(code as u32))
            .await?;

        *self.codec.lock().await = None;

        Ok(())
    }
}
