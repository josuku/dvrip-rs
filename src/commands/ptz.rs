use crate::constants::{KEY_CODES, OK_CODES};
use crate::dvrip::DVRIPCam;
use crate::error::Result;
use async_trait::async_trait;
use serde_json::json;
use strum_macros::AsRefStr;
use tokio::time::{Duration, sleep};

#[derive(Debug, Clone, Copy, AsRefStr)]
pub enum PTZCommand {
    DirectionUp,
    DirectionDown,
    DirectionLeft,
    DirectionRight,
    DirectionLeftUp,
    DirectionLeftDown,
    DirectionRightUp,
    DirectionRightDown,
    ZoomTile,
    ZoomWide,
    FocusNear,
    FocusFar,
    IrisSmall,
    IrisLarge,
    SetPreset,
    GotoPreset,
    ClearPreset,
    StartTour,
    StopTour,
}

#[async_trait]
pub trait PTZ: Send + Sync {
    /// Control PTZ with continuous command
    async fn ptz(&self, cmd: PTZCommand, step: u8, preset: i32, channel: u8) -> Result<bool>;

    /// Control PTZ with single step movement
    async fn ptz_step(&self, cmd: PTZCommand, step: u8) -> Result<bool>;

    async fn ptz_start(&self, cmd: PTZCommand, step: u8) -> Result<bool>;
    async fn ptz_stop(&self, cmd: PTZCommand, step: u8) -> Result<bool>;

    /// Press a key (keyDown)
    async fn key_down(&self, key: &str) -> Result<bool>;

    /// Release a key (keyUp)
    async fn key_up(&self, key: &str) -> Result<bool>;

    /// Press and release a key
    async fn key_press(&self, key: &str) -> Result<bool>;

    /// Execute a key script
    async fn key_script(&self, keys: &str) -> Result<bool>;
}

#[async_trait]
impl PTZ for DVRIPCam {
    async fn ptz(&self, cmd: PTZCommand, step: u8, preset: i32, channel: u8) -> Result<bool> {
        let cmd_str = cmd.as_ref().to_string();
        let ptz_param = json!({
            "AUX": {"Number": 0, "Status": "On"},
            "Channel": channel,
            "MenuOpts": "Enter",
            "Pattern": "Start",
            "Preset": preset,
            "Step": step,
            "Tour": if cmd_str.contains("Tour") { 1 } else { 0 },
        });

        let data = json!({
            "Command": cmd_str,
            "Parameter": ptz_param,
        });

        let reply = self.set_command("OPPTZControl", data, None).await?;
        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64()) {
            return Ok(OK_CODES.contains(&(ret as u32)));
        }
        Ok(false)
    }

    async fn ptz_start(&self, cmd: PTZCommand, step: u8) -> Result<bool> {
        let cmd_str = cmd.as_ref().to_string();

        // Start Movement
        let params_start = json!({
            "AUX": {"Number": 0, "Status": "On"},
            "Channel": 0,
            "MenuOpts": "Enter",
            "POINT": {"bottom": 0, "left": 0, "right": 0, "top": 0},
            "Pattern": "SetBegin",
            "Preset": 65535,
            "Step": step,
            "Tour": 0,
        });

        let data_start = json!({
            "Command": cmd_str,
            "Parameter": params_start,
        });

        self.set_command("OPPTZControl", data_start, None).await?;
        Ok(false)
    }
    async fn ptz_stop(&self, cmd: PTZCommand, step: u8) -> Result<bool> {
        let cmd_str = cmd.as_ref().to_string();

        let params_end = json!({
            "AUX": {"Number": 0, "Status": "On"},
            "Channel": 0,
            "MenuOpts": "Enter",
            "POINT": {"bottom": 0, "left": 0, "right": 0, "top": 0},
            "Pattern": "SetBegin",
            "Preset": -1,
            "Step": step,
            "Tour": 0,
        });

        let data_end = json!({
            "Command": cmd_str,
            "Parameter": params_end,
        });

        let reply = self.set_command("OPPTZControl", data_end, None).await?;
        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64()) {
            return Ok(OK_CODES.contains(&(ret as u32)));
        }
        Ok(false)
    }

    async fn ptz_step(&self, cmd: PTZCommand, step: u8) -> Result<bool> {
        let cmd_str = cmd.as_ref().to_string();

        // Start Movement
        let params_start = json!({
            "AUX": {"Number": 0, "Status": "On"},
            "Channel": 0,
            "MenuOpts": "Enter",
            "POINT": {"bottom": 0, "left": 0, "right": 0, "top": 0},
            "Pattern": "SetBegin",
            "Preset": 65535,
            "Step": step,
            "Tour": 0,
        });

        let data_start = json!({
            "Command": cmd_str,
            "Parameter": params_start,
        });

        self.set_command("OPPTZControl", data_start, None).await?;

        // Stop movement
        let params_end = json!({
            "AUX": {"Number": 0, "Status": "On"},
            "Channel": 0,
            "MenuOpts": "Enter",
            "POINT": {"bottom": 0, "left": 0, "right": 0, "top": 0},
            "Pattern": "SetBegin",
            "Preset": -1,
            "Step": step,
            "Tour": 0,
        });

        let data_end = json!({
            "Command": cmd_str,
            "Parameter": params_end,
        });

        let reply = self.set_command("OPPTZControl", data_end, None).await?;
        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64()) {
            return Ok(OK_CODES.contains(&(ret as u32)));
        }
        Ok(false)
    }

    async fn key_down(&self, key: &str) -> Result<bool> {
        let data = json!({
            "Status": "KeyDown",
            "Value": key,
        });

        let reply = self.set_command("OPNetKeyboard", data, None).await?;
        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64()) {
            return Ok(OK_CODES.contains(&(ret as u32)));
        }
        Ok(false)
    }

    async fn key_up(&self, key: &str) -> Result<bool> {
        let data = json!({
            "Status": "KeyUp",
            "Value": key,
        });

        let reply = self.set_command("OPNetKeyboard", data, None).await?;
        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64()) {
            return Ok(OK_CODES.contains(&(ret as u32)));
        }
        Ok(false)
    }

    async fn key_press(&self, key: &str) -> Result<bool> {
        self.key_down(key).await?;
        sleep(Duration::from_millis(300)).await;
        self.key_up(key).await
    }

    async fn key_script(&self, keys: &str) -> Result<bool> {
        for k in keys.chars() {
            if k != ' ' {
                let key_upper = k.to_uppercase().to_string();
                if let Some(key_code) = KEY_CODES.get(key_upper.as_str()) {
                    self.key_press(key_code).await?;
                }
            } else {
                sleep(Duration::from_secs(1)).await;
            }
        }
        Ok(true)
    }
}
