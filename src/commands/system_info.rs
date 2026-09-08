use crate::constants::DATE_FORMAT;
use crate::dvrip::DVRIPCam;
use crate::error::Result;
use async_trait::async_trait;
use chrono::{DateTime, Local, NaiveDateTime};
use serde_json::Value;

#[async_trait]
pub trait SystemInfo: Send + Sync {
    /// Get general system information
    async fn get_system_info(&self) -> Result<Value>;

    /// Get general information
    async fn get_general_info(&self) -> Result<Value>;

    /// Get network information
    async fn get_network_info(&self) -> Result<Value>;

    /// Get encoding capabilities
    async fn get_encode_capabilities(&self) -> Result<Value>;

    /// Get system capabilities
    async fn get_system_capabilities(&self) -> Result<Value>;

    /// Get camera information
    async fn get_camera_info(&self, default_config: bool) -> Result<Value>;

    /// Get encoding information
    async fn get_encode_info(&self, default_config: bool) -> Result<Value>;

    /// Get current device time
    async fn get_time(&self) -> Result<DateTime<Local>>;

    /// Set device time
    async fn set_time(&self, time: Option<DateTime<Local>>) -> Result<bool>;

    /// Get channel titles
    async fn get_channel_titles(&self) -> Result<Vec<String>>;

    /// Set channel titles
    async fn set_channel_titles(&self, titles: Vec<String>) -> Result<bool>;

    /// Get channel statuses
    async fn get_channel_statuses(&self) -> Result<Value>;
}

#[async_trait]
impl SystemInfo for DVRIPCam {
    async fn get_system_info(&self) -> Result<Value> {
        self.get_command("SystemInfo", None).await
    }

    async fn get_general_info(&self) -> Result<Value> {
        self.get_command("General", None).await
    }

    async fn get_network_info(&self) -> Result<Value> {
        self.get_command("NetWork.NetCommon", None).await
    }

    async fn get_encode_capabilities(&self) -> Result<Value> {
        self.get_command("EncodeCapability", None).await
    }

    async fn get_system_capabilities(&self) -> Result<Value> {
        self.get_command("SystemFunction", None).await
    }

    async fn get_camera_info(&self, default_config: bool) -> Result<Value> {
        let code = if default_config {
            Some(1044)
        } else {
            Some(1042)
        };
        self.get_command("Camera", code).await
    }

    async fn get_encode_info(&self, default_config: bool) -> Result<Value> {
        let code = if default_config {
            Some(1044)
        } else {
            Some(1042)
        };
        self.get_command("Simplify.Encode", code).await
    }

    async fn get_time(&self) -> Result<DateTime<Local>> {
        let time_str = self
            .get_command("OPTimeQuery", None)
            .await?
            .as_str()
            .ok_or_else(|| {
                crate::error::DVRIPError::ProtocolError("Invalid time response".to_string())
            })?
            .to_string();

        let naive = NaiveDateTime::parse_from_str(&time_str, DATE_FORMAT).map_err(|e| {
            crate::error::DVRIPError::ProtocolError(format!("Error parsing date: {}", e))
        })?;

        Ok(DateTime::from_naive_utc_and_offset(
            naive,
            *Local::now().offset(),
        ))
    }

    async fn set_time(&self, time: Option<DateTime<Local>>) -> Result<bool> {
        let time_to_set = time.unwrap_or_else(Local::now);
        let time_str = time_to_set.format(DATE_FORMAT).to_string();

        let reply = self
            .set_command("OPTimeSetting", serde_json::json!(time_str), None)
            .await?;
        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64()) {
            return Ok(crate::constants::OK_CODES.contains(&(ret as u32)));
        }
        Ok(false)
    }

    async fn get_channel_titles(&self) -> Result<Vec<String>> {
        let data = self.get_command("ChannelTitle", Some(1048)).await?;
        if let Some(titles) = data.as_array() {
            return Ok(titles
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect());
        }
        Ok(vec![])
    }

    async fn set_channel_titles(&self, titles: Vec<String>) -> Result<bool> {
        let session = self.session_id();
        let data = serde_json::json!({
            "ChannelTitle": titles,
            "Name": "ChannelTitle",
            "SessionID": format!("0x{:08X}", session),
        });

        let reply = self.set_command("ChannelTitle", data, None).await?;
        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64()) {
            return Ok(crate::constants::OK_CODES.contains(&(ret as u32)));
        }
        Ok(false)
    }

    async fn get_channel_statuses(&self) -> Result<Value> {
        self.get_command("NetWork.ChnStatus", None).await
    }
}
