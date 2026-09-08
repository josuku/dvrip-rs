use crate::constants::{OK_CODES, QCODES};
use crate::dvrip::DVRIPCam;
use crate::error::Result;
use async_trait::async_trait;
use byteorder::{BigEndian, ByteOrder, LittleEndian};
use serde_json::json;
use std::sync::atomic::Ordering;
use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameMetadata {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<u8>,
    pub frame_type: Option<String>,
    pub media_type: Option<String>,
    pub datetime: Option<chrono::DateTime<chrono::Local>>,
}

pub type FrameCallback = Box<dyn Fn(Vec<u8>, FrameMetadata) + Send + Sync>;

#[async_trait]
pub trait Monitoring: Send + Sync {
    /// Start video monitoring
    async fn start_monitor(
        &self,
        stream: &str,
        channel: u8,
    ) -> Result<broadcast::Receiver<(FrameMetadata, Vec<u8>)>>;

    /// Stop video monitoring
    async fn stop_monitor(&self) -> Result<()>;

    /// Get a snapshot (screenshot)
    async fn snapshot(&self, channel: u8) -> Result<Vec<u8>>;

    /// Check if monitoring
    fn is_monitoring(&self) -> bool;
}

#[async_trait]
impl Monitoring for DVRIPCam {
    async fn start_monitor(
        &self,
        stream: &str,
        channel: u8,
    ) -> Result<broadcast::Receiver<(FrameMetadata, Vec<u8>)>> {
        let params = json!({
            "Channel": channel,
            "CombinMode": "NONE",
            "StreamType": stream,
            "TransMode": "TCP",
        });

        let data = json!({
            "Action": "Claim",
            "Parameter": params,
        });

        let reply = self.set_command("OPMonitor", data, None).await?;
        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64())
            && !OK_CODES.contains(&(ret as u32))
        {
            return Err(crate::error::DVRIPError::ProtocolError(
                "Failed to start monitoring".to_string(),
            ));
        }

        let session = self.session_id();
        let start_data = json!({
            "Name": "OPMonitor",
            "SessionID": format!("0x{:08X}", session),
            "OPMonitor": {
                "Action": "Start",
                "Parameter": params,
            },
        });

        self.send_command(1410, start_data, false).await?;
        self.monitoring.store(true, Ordering::Release);

        Ok(self.frame_sender.subscribe())
    }

    async fn stop_monitor(&self) -> Result<()> {
        self.monitoring.store(false, Ordering::Release);
        Ok(())
    }

    async fn snapshot(&self, channel: u8) -> Result<Vec<u8>> {
        let session = self.session_id();
        let data = json!({
            "Name": "OPSNAP",
            "SessionID": format!("0x{:08X}", session),
            "OPSNAP": {
                "Channel": channel,
            },
        });

        let data = self
            .send_command_recv_bin(QCODES.get("OPSNAP").copied().unwrap_or(1560), data, true)
            .await?;

        if let Some(s) = data {
            let (frame, _) = DVRIPCam::read_bin_payload_static(s).await?;
            return Ok(frame);
        }

        Err(crate::error::DVRIPError::ConnectionError(
            "Stream not available".to_string(),
        ))
    }

    fn is_monitoring(&self) -> bool {
        self.monitoring.load(Ordering::Acquire)
    }
}

impl DVRIPCam {
    pub(crate) async fn read_bin_payload_static(
        packet: Vec<u8>,
    ) -> Result<(Vec<u8>, FrameMetadata)> {
        let mut metadata = FrameMetadata {
            width: None,
            height: None,
            fps: None,
            frame_type: None,
            media_type: None,
            datetime: None,
        };
        let mut buf: Vec<u8> = vec![];
        let mut length = 0u32;
        let frame_len;

        let data_type = BigEndian::read_u32(&packet[0..4]);
        if data_type == 0x1FC || data_type == 0x1FE {
            frame_len = 16;
            if packet.len() >= frame_len {
                let media = packet[4];
                metadata.fps = Some(packet[5]);
                let w = packet[6] as u32;
                let h = packet[7] as u32;
                let dt = LittleEndian::read_u32(&packet[8..12]);
                length = LittleEndian::read_u32(&packet[12..16]);

                metadata.width = Some(w * 8);
                metadata.height = Some(h * 8);
                metadata.datetime = Some(Self::internal_to_datetime_static(dt));

                if data_type == 0x1FC {
                    metadata.frame_type = Some("I".to_string());
                }

                metadata.media_type = Self::internal_to_type_static(data_type, media);
            }
        } else if data_type == 0x1FD {
            frame_len = 8;
            if packet.len() >= frame_len {
                length = LittleEndian::read_u32(&packet[4..8]);
                metadata.frame_type = Some("P".to_string());
            }
        } else if data_type == 0x1FA {
            frame_len = 8;
            if packet.len() >= frame_len {
                let media = packet[4];
                let _samp_rate = LittleEndian::read_u16(&packet[5..7]);
                length = LittleEndian::read_u16(&packet[6..8]) as u32;
                metadata.media_type = Self::internal_to_type_static(data_type, media);
            }
        } else if data_type == 0x1F9 {
            frame_len = 8;
            if packet.len() >= frame_len {
                let media = packet[4];
                let _n = packet[5];
                length = LittleEndian::read_u16(&packet[6..8]) as u32;
                metadata.media_type = Self::internal_to_type_static(data_type, media);
            }
        } else if data_type == 0xFFD8FFE0 {
            return Ok((packet, metadata));
        } else {
            return Err(crate::error::DVRIPError::ProtocolError(format!(
                "Unknown data type: 0x{:X}",
                data_type
            )));
        }
        if frame_len < packet.len() {
            buf.extend_from_slice(&packet[frame_len..]);
        }

        buf.truncate(length as usize);
        Ok((buf, metadata))
    }

    fn internal_to_type_static(data_type: u32, value: u8) -> Option<String> {
        match data_type {
            0x1FC | 0x1FD => match value {
                1 => Some("mpeg4".to_string()),
                2 => Some("h264".to_string()),
                3 => Some("h265".to_string()),
                _ => None,
            },
            0x1F9 => {
                if value == 1 || value == 6 {
                    Some("info".to_string())
                } else {
                    None
                }
            }
            0x1FA => {
                if value == 0xE {
                    Some("g711a".to_string())
                } else {
                    None
                }
            }
            0x1FE => {
                if value == 0 {
                    Some("jpeg".to_string())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn internal_to_datetime_static(value: u32) -> chrono::DateTime<chrono::Local> {
        let second = value & 0x3F;
        let minute = (value & 0xFC0) >> 6;
        let hour = (value & 0x1F000) >> 12;
        let day = (value & 0x3E0000) >> 17;
        let month = (value & 0x3C00000) >> 22;
        let year = ((value & 0xFC000000) >> 26) + 2000;

        chrono::NaiveDate::from_ymd_opt(year as i32, month, day)
            .and_then(|d| d.and_hms_opt(hour, minute, second))
            .map(|dt| {
                chrono::DateTime::from_naive_utc_and_offset(dt, *chrono::Local::now().offset())
            })
            .unwrap_or_else(chrono::Local::now)
    }
}
