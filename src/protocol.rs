use crate::error::{DVRIPError, Result};
use byteorder::{ByteOrder, LittleEndian};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub struct PacketHeader {
    pub head: u8,
    pub version: u8,
    pub session: u32,
    pub packet_count: u32,
    pub msg_id: u16,
    pub data_len: u32,
}

impl PacketHeader {
    pub const SIZE: usize = 20;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = vec![0u8; Self::SIZE];
        buf[0] = self.head;
        buf[1] = self.version;
        LittleEndian::write_u32(&mut buf[4..8], self.session);
        LittleEndian::write_u32(&mut buf[8..12], self.packet_count);
        LittleEndian::write_u16(&mut buf[14..16], self.msg_id);
        LittleEndian::write_u32(&mut buf[16..20], self.data_len);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            return Err(DVRIPError::ProtocolError("Header too small".to_string()));
        }
        Ok(Self {
            head: data[0],
            version: data[1],
            session: LittleEndian::read_u32(&data[4..8]),
            packet_count: LittleEndian::read_u32(&data[8..12]),
            msg_id: LittleEndian::read_u16(&data[14..16]),
            data_len: LittleEndian::read_u32(&data[16..20]),
        })
    }
}

pub async fn pack_packet(
    session: u32,
    packet_count: u32,
    msg_id: u16,
    data: &[u8],
    version: u8,
    add_tail: bool,
) -> Result<(PacketHeader, Vec<u8>)> {
    let tail: &[u8] = if !add_tail {
        b""
    } else if version == 0 {
        b"\x0a\x00"
    } else {
        b"\x00"
    };
    let data_len = (data.len() + tail.len()) as u32;

    let header = PacketHeader {
        head: 255,
        version,
        session,
        packet_count,
        msg_id,
        data_len,
    };

    let mut result = vec![];
    result.extend_from_slice(data);
    result.extend_from_slice(tail);
    Ok((header, result))
}

pub async fn write_packet<W: AsyncWrite + Unpin>(
    writer: &mut W,
    session: u32,
    packet_count: u32,
    msg_id: u16,
    data: &[u8],
    version: u8,
) -> Result<()> {
    let tail: &[u8] = if version == 0 { b"\x0a\x00" } else { b"\x00" };
    let data_len = (data.len() + tail.len()) as u32;

    let header = PacketHeader {
        head: 255,
        version,
        session,
        packet_count,
        msg_id,
        data_len,
    };

    let mut packet = header.encode();
    packet.extend_from_slice(data);
    packet.extend_from_slice(tail);

    writer.write_all(&packet).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn receive_packet_header<R: AsyncRead + Unpin>(reader: &mut R) -> Result<PacketHeader> {
    let mut buf = vec![0u8; PacketHeader::SIZE];
    let mut received = 0;

    // Read header in parts to avoid issues with data not immediately available
    while received < PacketHeader::SIZE {
        match reader.read(&mut buf[received..]).await {
            Ok(0) => {
                return Err(DVRIPError::ConnectionError(
                    "Connection closed by peer".to_string(),
                ));
            }
            Ok(n) => {
                received += n;
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Err(DVRIPError::ConnectionError(
                    "Connection closed unexpectedly".to_string(),
                ));
            }
            Err(e) => {
                return Err(DVRIPError::IoError(e));
            }
        }
    }

    PacketHeader::decode(&buf)
}

pub async fn receive_data<R: AsyncRead + Unpin>(
    reader: &mut R,
    length: usize,
    timeout: tokio::time::Duration,
) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; length];
    let mut received = 0;

    while received < length {
        let remaining = length - received;
        let result = tokio::time::timeout(
            timeout,
            reader.read(&mut buf[received..received + remaining]),
        )
        .await;

        let chunk = match result {
            Ok(Ok(n)) => n,
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Err(DVRIPError::ConnectionError(
                    "Connection closed unexpectedly during read".to_string(),
                ));
            }
            Ok(Err(e)) => {
                return Err(DVRIPError::IoError(e));
            }
            Err(_) => {
                return Err(DVRIPError::ConnectionError(
                    "Timeout receiving data".to_string(),
                ));
            }
        };

        if chunk == 0 {
            return Err(DVRIPError::ConnectionError(
                "Connection closed by peer".to_string(),
            ));
        }
        received += chunk;
    }

    Ok(buf)
}

pub async fn unpack_json(data: &[u8]) -> Result<Value> {
    let json_data =
        if data.len() >= 2 && data[data.len() - 2] == 0x0a && data[data.len() - 1] == 0x00 {
            &data[..data.len() - 2]
        } else if !data.is_empty() && data[data.len() - 1] == 0x00 {
            &data[..data.len() - 1]
        } else {
            data
        };

    let json_str = String::from_utf8_lossy(json_data);
    serde_json::from_str(&json_str)
        .map_err(|e| DVRIPError::SerializationError(format!("Error parsing JSON: {}", e)))
}

pub async fn receive_json<R: AsyncRead + Unpin>(
    reader: &mut R,
    length: usize,
    timeout: tokio::time::Duration,
) -> Result<Value> {
    let data = receive_data(reader, length, timeout).await?;
    // Remove tail (\x0a\x00 or \x00)
    let json_data =
        if data.len() >= 2 && data[data.len() - 2] == 0x0a && data[data.len() - 1] == 0x00 {
            &data[..data.len() - 2]
        } else if !data.is_empty() && data[data.len() - 1] == 0x00 {
            &data[..data.len() - 1]
        } else {
            &data
        };

    let json_str = String::from_utf8_lossy(json_data);
    serde_json::from_str(&json_str)
        .map_err(|e| DVRIPError::SerializationError(format!("Error parsing JSON: {}", e)))
}

pub fn sofia_hash(password: &str) -> String {
    let digest = md5::compute(password.as_bytes());

    let chars: Vec<char> = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"
        .chars()
        .collect();

    let mut result = String::new();
    for i in (0..digest.len()).step_by(2) {
        if i + 1 < digest.len() {
            let sum = digest[i] as usize + digest[i + 1] as usize;
            result.push(chars[sum % 62]);
        }
    }
    result
}
