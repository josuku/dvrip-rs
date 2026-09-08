use crate::commands::Connection;
use crate::constants::{OK_CODES, QCODES};
use crate::dvrip::DVRIPCam;
use crate::error::Result;
use crate::protocol::sofia_hash;
use async_trait::async_trait;
use serde_json::json;
use std::sync::atomic::Ordering;

#[async_trait]
pub trait Authentication: Send + Sync {
    /// Login to the device
    async fn login(&mut self, username: &str, password: &str) -> Result<bool>;

    /// Logout from the device
    async fn logout(&mut self) -> Result<()>;

    /// Check if authenticated
    fn is_authenticated(&self) -> bool;

    /// Get the session ID
    fn session_id(&self) -> u32;

    /// Change user password
    async fn change_password(
        &self,
        old_password: &str,
        new_password: &str,
        username: Option<&str>,
    ) -> Result<bool>;
}

#[async_trait]
impl Authentication for DVRIPCam {
    async fn login(&mut self, username: &str, password: &str) -> Result<bool> {
        if !Connection::is_connected(self) {
            Connection::connect(self, self.timeout).await?;
        }

        let data = json!({
            "EncryptType": "MD5",
            "LoginType": "DVRIP-Web",
            "PassWord": sofia_hash(password),
            "UserName": username,
        });
        self.username = Some(username.to_string());

        let reply = self.send_command(1000, data, true).await?.ok_or_else(|| {
            crate::error::DVRIPError::AuthenticationError("Empty response".to_string())
        })?;

        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64())
            && OK_CODES.contains(&(ret as u32))
        {
            if let Some(session_str) = reply.get("SessionID").and_then(|s| s.as_str()) {
                let session_id = u32::from_str_radix(&session_str[2..], 16).map_err(|_| {
                    crate::error::DVRIPError::ProtocolError("Invalid SessionID".to_string())
                })?;
                self.session.store(session_id, Ordering::Release);
            }

            if let Some(interval) = reply.get("AliveInterval").and_then(|i| i.as_u64()) {
                self.alive_time.store(interval, Ordering::Release);
            }

            self.authenticated.store(true, Ordering::Release);
            self.start_keep_alive().await;
            return Ok(true);
        }

        Ok(false)
    }

    async fn logout(&mut self) -> Result<()> {
        Connection::close(self).await
    }

    fn is_authenticated(&self) -> bool {
        self.authenticated.load(Ordering::Acquire)
    }

    fn session_id(&self) -> u32 {
        self.session.load(Ordering::Acquire)
    }

    async fn change_password(
        &self,
        old_password: &str,
        new_password: &str,
        username: Option<&str>,
    ) -> Result<bool> {
        let data = json!({
            "EncryptType": "MD5",
            "NewPassWord": sofia_hash(new_password),
            "PassWord": sofia_hash(old_password),
            "SessionID": format!("0x{:08X}", self.session_id()),
            "UserName": username.unwrap_or(self.username.as_ref().unwrap_or(&"admin".to_string())),
        });

        let reply = self
            .send_command(
                QCODES.get("ModifyPassword").copied().unwrap_or(1488),
                data,
                true,
            )
            .await?
            .ok_or_else(|| crate::error::DVRIPError::ProtocolError("Empty response".to_string()))?;

        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64()) {
            return Ok(OK_CODES.contains(&(ret as u32)));
        }

        Ok(false)
    }
}
