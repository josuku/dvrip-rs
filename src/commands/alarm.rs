use crate::error::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::constants::{OK_CODES, QCODES};
use crate::dvrip::DVRIPCam;
use std::sync::atomic::Ordering;

pub type AlarmCallback = Box<dyn Fn(Value, u32) + Send + Sync>;

#[async_trait]
pub trait Alarm: Send + Sync {
    /// Set the alarm callback function
    fn set_alarm_callback(&self, callback: Option<AlarmCallback>);

    /// Clear the alarm callback
    fn clear_alarm_callback(&self);

    /// Start alarm monitoring
    async fn start_alarm_monitoring(&self) -> Result<()>;

    /// Stop alarm monitoring
    async fn stop_alarm_monitoring(&self) -> Result<()>;

    /// Set remote alarm
    async fn set_remote_alarm(&self, state: bool) -> Result<bool>;

    /// Check if monitoring alarms
    fn is_alarm_monitoring(&self) -> bool;
}

#[async_trait]
impl Alarm for DVRIPCam {
    fn set_alarm_callback(&self, callback: Option<AlarmCallback>) {
        let alarm_cb = self.alarm_callback.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                *alarm_cb.lock().await = callback;
            });
        } else {
            // If not in an async context, create a temporary runtime
            tokio::spawn(async move {
                *alarm_cb.lock().await = callback;
            });
        }
    }

    fn clear_alarm_callback(&self) {
        let alarm_cb = self.alarm_callback.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                *alarm_cb.lock().await = None;
            });
        } else {
            tokio::spawn(async move {
                *alarm_cb.lock().await = None;
            });
        }
    }

    async fn start_alarm_monitoring(&self) -> Result<()> {
        let reply = self
            .get_command(
                "",
                Some(QCODES.get("AlarmSet").copied().unwrap_or(1500) as u32),
            )
            .await?;

        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64())
            && !OK_CODES.contains(&(ret as u32))
        {
            return Err(crate::error::DVRIPError::ProtocolError(
                "Failed to start alarm monitoring".to_string(),
            ));
        }

        self.alarm_monitoring.store(true, Ordering::Release);

        Ok(())
    }

    async fn stop_alarm_monitoring(&self) -> Result<()> {
        self.alarm_monitoring.store(false, Ordering::Release);

        Ok(())
    }

    async fn set_remote_alarm(&self, state: bool) -> Result<bool> {
        let data = serde_json::json!({
            "Event": 0,
            "State": state,
        });

        let reply = self.set_command("OPNetAlarm", data, None).await?;
        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64()) {
            return Ok(OK_CODES.contains(&(ret as u32)));
        }
        Ok(false)
    }

    fn is_alarm_monitoring(&self) -> bool {
        self.alarm_monitoring.load(Ordering::Acquire)
    }
}
