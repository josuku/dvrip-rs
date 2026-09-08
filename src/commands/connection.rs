use crate::constants::QCODES;
use crate::dvrip::DVRIPCam;
use crate::error::Result;
use crate::protocol::PacketHeader;
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync;
use tokio::time::Duration;

#[async_trait]
pub trait Connection: Send + Sync {
    /// Connect to the device
    async fn connect(&mut self, timeout: tokio::time::Duration) -> Result<()>;

    /// Disconnect from the device
    async fn close(&mut self) -> Result<()>;

    /// Check if connected
    fn is_connected(&self) -> bool;

    /// Get the device IP address
    fn ip(&self) -> &str;

    /// Get the device port
    fn port(&self) -> u16;
}

#[async_trait]
impl Connection for DVRIPCam {
    async fn connect(&mut self, timeout: Duration) -> Result<()> {
        self.timeout = timeout;

        let stream: TcpStream =
            tokio::time::timeout(timeout, TcpStream::connect((self.ip.as_str(), self.port)))
                .await
                .map_err(|_| {
                    crate::error::DVRIPError::ConnectionError("Connection timeout".to_string())
                })?
                .map_err(|e| {
                    crate::error::DVRIPError::ConnectionError(format!("Connection error: {}", e))
                })?;

        let (mut read, mut write) = stream.into_split();

        let message_handlers: Arc<
            DashMap<u32, tokio::sync::oneshot::Sender<(PacketHeader, Vec<u8>)>>,
        > = Arc::new(DashMap::new());

        let ptr_1 = Arc::clone(&message_handlers);
        let alarm_callback = Arc::clone(&self.alarm_callback);
        let frame_channel = Arc::clone(&self.frame_sender);
        let monitoring = Arc::clone(&self.alarm_monitoring);
        let video_monitoring = Arc::clone(&self.monitoring);
        let stream_handlers = Arc::clone(&self.stream_handlers);

        *self.recv_handle.lock().await = Some(tokio::spawn(async move {
            let alarm_info_code = QCODES.get("AlarmInfo").copied().unwrap_or(1504);
            loop {
                let mut header = [0u8; 20];
                read.read_exact(&mut header)
                    .await
                    .expect("Error reading packet header");
                let decoded_header = PacketHeader::decode(&header).unwrap();

                let mut data = vec![0u8; decoded_header.data_len as usize];
                read.read_exact(&mut data)
                    .await
                    .expect("Error reading packet data");

                if decoded_header.msg_id == 1412 && video_monitoring.load(Ordering::Acquire) {
                    DVRIPCam::__handle_video(frame_channel.clone(), data).await;
                    continue;
                }

                if decoded_header.msg_id == alarm_info_code && monitoring.load(Ordering::Acquire) {
                    DVRIPCam::__handle_alarm(Arc::clone(&alarm_callback), decoded_header, data)
                        .await;
                    continue;
                }

                if let Some((_, handler)) = ptr_1.remove(&decoded_header.packet_count) {
                    let _ = handler.send((decoded_header, data));
                    continue;
                }

                if let Some(handler) = stream_handlers.get(&decoded_header.msg_id) {
                    let _ = handler.send((decoded_header, data)).await;
                }
            }
        }));

        let (send, mut recv) = sync::mpsc::channel(100);
        self.send_pool = Arc::new(Some(send));
        *self.send_handle.lock().await = Some(tokio::spawn(async move {
            let mut packet_count = 1;
            while let Some(request) = recv.recv().await {
                let mut header = request.header;
                let use_internal_counter = request.use_internal_counter;

                if use_internal_counter {
                    header.packet_count = packet_count;
                }

                // If a response sender is provided, wait for the response
                if let Some(sender) = request.response_sender {
                    message_handlers.insert(
                        // 0x0585 is the code for starting the stream
                        // i don't really know why the packet count for this specifically has to be one more but ok
                        if header.msg_id == 0x0585
                            || header.msg_id == 0x590
                            || header.msg_id == 0x059a
                        {
                            header.packet_count + 1
                        } else {
                            header.packet_count
                        },
                        sender,
                    );
                }

                // Send the packet
                write
                    .write_all(&header.encode())
                    .await
                    .expect("Error sending packet header. Cannot continue.");
                write
                    .write_all(&request.data)
                    .await
                    .expect("Error sending packet data. Cannot continue.");
                write.flush().await.unwrap();

                if use_internal_counter {
                    packet_count += 1;
                }
            }
        }));

        self.connected.store(true, Ordering::Release);

        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        self.connected.store(false, Ordering::Release);
        self.authenticated.store(false, Ordering::Release);
        self.monitoring.store(false, Ordering::Release);
        self.alarm_monitoring.store(false, Ordering::Release);

        // Cancel background tasks
        if let Some(handle) = self.keep_alive_handle.lock().await.take() {
            handle.abort();
        }
        // Removed alarm_handle cancellation as it's no longer used
        if let Some(handle) = self.recv_handle.lock().await.take() {
            handle.abort();
        }
        if let Some(handle) = self.send_handle.lock().await.take() {
            handle.abort();
        }

        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    fn ip(&self) -> &str {
        &self.ip
    }

    fn port(&self) -> u16 {
        self.port
    }
}
