use crate::commands::AlarmCallback;
use crate::constants::{OK_CODES, QCODES, TCP_PORT};
use crate::error::{DVRIPError, Result};
use crate::protocol::{PacketHeader, pack_packet, unpack_json};
use crate::{AudioCodec, FrameMetadata};
use dashmap::DashMap;
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use tokio::sync::{self, Mutex, broadcast, mpsc};
use tokio::time::Duration;

pub struct CommandRequest {
    pub header: PacketHeader,
    pub data: Vec<u8>,
    pub response_sender: Option<tokio::sync::oneshot::Sender<(PacketHeader, Vec<u8>)>>,
    pub use_internal_counter: bool,
    pub expected_response_id: Option<u16>,
}

impl CommandRequest {
    pub fn new(header: PacketHeader, data: Vec<u8>) -> Self {
        Self {
            header,
            data,
            response_sender: None,
            use_internal_counter: true,
            expected_response_id: None,
        }
    }

    pub fn with_response(
        mut self,
        sender: tokio::sync::oneshot::Sender<(PacketHeader, Vec<u8>)>,
    ) -> Self {
        self.response_sender = Some(sender);
        self
    }

    pub fn with_counter(mut self, use_internal: bool) -> Self {
        self.use_internal_counter = use_internal;
        self
    }

    pub fn with_expected_response(mut self, id: u16) -> Self {
        self.expected_response_id = Some(id);
        self
    }
}

pub struct DVRIPCam {
    pub(crate) ip: String,
    pub(crate) port: u16,
    pub(crate) timeout: Duration,

    pub(crate) username: Option<String>,

    // Atomic state
    pub(crate) connected: Arc<AtomicBool>,
    pub(crate) authenticated: Arc<AtomicBool>,
    pub(crate) monitoring: Arc<AtomicBool>,
    pub(crate) alarm_monitoring: Arc<AtomicBool>,

    // Atomic counters
    pub(crate) session: Arc<AtomicU32>,

    // Callbacks
    pub(crate) alarm_callback: Arc<Mutex<Option<AlarmCallback>>>,
    pub(crate) frame_sender: Arc<broadcast::Sender<(FrameMetadata, Vec<u8>)>>,

    // Background tasks
    pub(crate) keep_alive_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    pub(crate) recv_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    pub(crate) send_handle: Arc<Mutex<Option<JoinHandle<()>>>>,

    // Stream handlers for persistent listeners (e.g. file download)
    pub(crate) stream_handlers: Arc<DashMap<u16, mpsc::Sender<(PacketHeader, Vec<u8>)>>>,

    // Configuration
    pub(crate) alive_time: Arc<AtomicU64>,

    pub(crate) codec: Arc<Mutex<Option<AudioCodec>>>,
    pub(crate) backchannel_buffer: Arc<Mutex<Vec<u8>>>,

    pub send_pool: Arc<Option<sync::mpsc::Sender<CommandRequest>>>,
}

impl DVRIPCam {
    pub fn new(ip: impl Into<String>) -> Self {
        let ip = ip.into();

        let (tx, _s) = broadcast::channel(25);

        Self {
            ip,
            username: None,
            port: TCP_PORT,
            codec: Arc::new(Mutex::new(None)),
            recv_handle: Arc::new(Mutex::new(None)),
            send_handle: Arc::new(Mutex::new(None)),
            frame_sender: Arc::new(tx),
            timeout: Duration::from_secs(10),
            connected: Arc::new(AtomicBool::new(false)),
            authenticated: Arc::new(AtomicBool::new(false)),
            monitoring: Arc::new(AtomicBool::new(false)),
            alarm_monitoring: Arc::new(AtomicBool::new(false)),
            session: Arc::new(AtomicU32::new(0)),
            alarm_callback: Arc::new(Mutex::new(None)),
            keep_alive_handle: Arc::new(Mutex::new(None)),
            alive_time: Arc::new(AtomicU64::new(20)),
            backchannel_buffer: Arc::new(Mutex::new(Vec::new())),
            send_pool: Arc::new(None),
            stream_handlers: Arc::new(DashMap::new()),
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn session_id(&self) -> u32 {
        self.session.load(Ordering::Acquire)
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub async fn __handle_video(
        frame_sender: Arc<broadcast::Sender<(FrameMetadata, Vec<u8>)>>,
        data: Vec<u8>,
    ) {
        let Ok((frame, metadata)) = DVRIPCam::read_bin_payload_static(data).await else {
            return;
        };

        frame_sender
            .send((metadata, frame))
            .expect("Failed to send frame");
    }

    pub async fn __handle_alarm(
        alarm_callback: Arc<tokio::sync::Mutex<Option<AlarmCallback>>>,
        decoded_header: PacketHeader,
        data: Vec<u8>,
    ) {
        if let Ok(data) = unpack_json(&data).await
            && let Some(ref callback) = *alarm_callback.lock().await
            && let Some(name) = data.get("Name").and_then(|n| n.as_str())
            && let Some(alarm_data) = data.get(name)
        {
            callback(alarm_data.clone(), decoded_header.packet_count);
        };
    }

    pub async fn send_raw_packet(
        &self,
        msg_id: u16,
        data: Vec<u8>,
        wait_response: bool,
        add_tail: bool,
    ) -> Result<Option<Vec<u8>>> {
        if !self.connected.load(Ordering::Acquire) {
            return Err(DVRIPError::ConnectionError("Not connected".to_string()));
        }

        let ptr = &*self.send_pool;
        let pool = ptr.clone().ok_or_else(|| {
            DVRIPError::ConnectionError("Did you connect to the camera?".to_string())
        })?;

        let session = self.session.load(Ordering::Acquire);

        let packed = pack_packet(session, 0, msg_id, &data, 0, add_tail).await?;

        let mut request = CommandRequest::new(packed.0, packed.1).with_counter(true);

        if wait_response {
            let (send, recv) = tokio::sync::oneshot::channel::<(PacketHeader, Vec<u8>)>();
            request = request.with_response(send);
            let _ = pool.send(request).await;

            let response = tokio::time::timeout(self.timeout, recv)
                .await
                .map_err(|_| {
                    DVRIPError::ConnectionError("Timeout waiting for response".to_string())
                })? // Timeout error
                .map_err(|_| {
                    DVRIPError::ConnectionError("Channel closed unexpectedly".to_string())
                })?; // RecvError

            return Ok(Some(response.1));
        }

        let _ = pool.send(request).await;
        Ok(None)
    }

    pub(crate) async fn send_command_recv_bin(
        &self,
        msg_id: u16,
        data: Value,
        wait_response: bool,
    ) -> Result<Option<Vec<u8>>> {
        let data_bytes = serde_json::to_string(&data)
            .map_err(|e| DVRIPError::SerializationError(e.to_string()))?
            .into_bytes();

        self.send_raw_packet(msg_id, data_bytes, wait_response, true)
            .await
    }

    pub(crate) async fn send_command(
        &self,
        msg_id: u16,
        data: Value,
        wait_response: bool,
    ) -> Result<Option<Value>> {
        let Some(data) = self
            .send_command_recv_bin(msg_id, data, wait_response)
            .await?
            .map(|x| serde_json::from_slice(&x[..x.len() - 2]))
        else {
            return Ok(None);
        };
        data.map_err(|_| DVRIPError::SerializationError("Failed to parse JSON Header".to_owned()))
    }

    pub(crate) async fn get_command(&self, command: &str, code: Option<u32>) -> Result<Value> {
        let msg_id =
            code.unwrap_or_else(|| QCODES.get(command).copied().unwrap_or(0).into()) as u16;

        let session = self.session.load(Ordering::Acquire);
        let data = json!({
            "Name": command,
            "SessionID": format!("0x{:08X}", session)
        });

        let reply = self
            .send_command(msg_id, data, true)
            .await?
            .ok_or_else(|| DVRIPError::ProtocolError("Empty response".to_string()))?;

        if let Some(ret) = reply.get("Ret")
            && let Some(ret_code) = ret.as_u64()
            && OK_CODES.contains(&(ret_code as u32))
            && let Some(cmd_data) = reply.get(command)
        {
            return Ok(cmd_data.clone());
        }

        Ok(reply)
    }

    pub(crate) async fn set_command(
        &self,
        command: &str,
        data: Value,
        code: Option<u32>,
    ) -> Result<Value> {
        let msg_id =
            code.unwrap_or_else(|| QCODES.get(command).copied().unwrap_or(0) as u32) as u16;

        let session = self.session.load(Ordering::Acquire);
        let mut cmd_data = json!({
            "Name": command,
            "SessionID": format!("0x{:08X}", session),
        });
        cmd_data[command] = data;

        let reply = self
            .send_command(msg_id, cmd_data, true)
            .await?
            .ok_or_else(|| DVRIPError::ProtocolError("Empty response".to_string()))?;

        Ok(reply)
    }

    pub(crate) async fn start_keep_alive(&self) {
        let session = self.session.clone();
        let alive_time = self.alive_time.clone();
        let stream = self.send_pool.clone();
        let connected = self.connected.clone();
        let _ = self.timeout;
        let keep_alive_code = QCODES.get("KeepAlive").copied().unwrap_or(1006);

        let handle = tokio::spawn(async move {
            loop {
                if !connected.load(Ordering::Acquire) {
                    break;
                }

                let interval = Duration::from_secs(alive_time.load(Ordering::Acquire));
                tokio::time::sleep(interval).await;

                let Some(s) = &*stream else {
                    connected.store(false, Ordering::Release);
                    break;
                };

                let session_id = session.load(Ordering::Acquire);
                let data = json!({
                    "Name": "KeepAlive",
                    "SessionID": format!("0x{:08X}", session_id)
                });

                let Ok(data_bytes) = serde_json::to_string(&data) else {
                    eprintln!("Failed to serialize keep-alive JSON");
                    continue;
                };

                if let Ok((header, body)) = pack_packet(
                    session_id,
                    0, // Keep alive can use fixed counter
                    keep_alive_code,
                    &data_bytes.into_bytes(),
                    0,
                    true,
                )
                .await
                {
                    let request = CommandRequest::new(header, body).with_counter(true);
                    let _ = s.send(request).await.map_err(|e| {
                        eprintln!("Failed to send keep-alive packet: {}", e);
                    });
                }
            }
        });

        *self.keep_alive_handle.lock().await = Some(handle);
    }
}
