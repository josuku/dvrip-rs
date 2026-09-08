use crate::constants::OK_CODES;
use crate::dvrip::DVRIPCam;
use crate::error::Result;
use crate::protocol::sofia_hash;
use async_trait::async_trait;
use serde_json::{Value, json};

#[async_trait]
pub trait UserManagement: Send + Sync {
    /// Get the list of authorities
    async fn get_authority_list(&self) -> Result<Vec<Value>>;

    /// Get the list of groups
    async fn get_groups(&self) -> Result<Vec<Value>>;

    /// Add a new group
    async fn add_group(&self, name: &str, comment: &str, auth: Option<Vec<Value>>) -> Result<bool>;

    /// Modify an existing group
    async fn modify_group(
        &self,
        name: &str,
        newname: Option<&str>,
        comment: Option<&str>,
        auth: Option<Vec<Value>>,
    ) -> Result<bool>;

    /// Delete a group
    async fn delete_group(&self, name: &str) -> Result<bool>;

    /// Get the list of users
    async fn get_users(&self) -> Result<Vec<Value>>;

    /// Add a new user
    async fn add_user(
        &self,
        name: &str,
        password: &str,
        comment: &str,
        group: &str,
        auth: Option<Vec<Value>>,
        sharable: bool,
    ) -> Result<bool>;

    /// Modify an existing user
    async fn modify_user(
        &self,
        name: &str,
        newname: Option<&str>,
        comment: Option<&str>,
        group: Option<&str>,
        auth: Option<Vec<Value>>,
        sharable: Option<bool>,
    ) -> Result<bool>;

    /// Delete a user
    async fn delete_user(&self, name: &str) -> Result<bool>;
}

#[async_trait]
impl UserManagement for DVRIPCam {
    async fn get_authority_list(&self) -> Result<Vec<Value>> {
        let data = self.get_command("AuthorityList", None).await?;
        if let Some(auth_list) = data.get("AuthorityList").and_then(|v| v.as_array()) {
            return Ok(auth_list.clone());
        }
        Ok(vec![])
    }

    async fn get_groups(&self) -> Result<Vec<Value>> {
        let data = self.get_command("Groups", None).await?;
        if let Some(groups) = data.get("Groups").and_then(|v| v.as_array()) {
            return Ok(groups.clone());
        }
        Ok(vec![])
    }

    async fn add_group(&self, name: &str, comment: &str, auth: Option<Vec<Value>>) -> Result<bool> {
        let auth_list = match auth {
            Some(a) => a,
            None => self.get_authority_list().await?,
        };

        let data = json!({
            "Group": {
                "AuthorityList": auth_list,
                "Memo": comment,
                "Name": name,
            }
        });

        let reply = self.set_command("AddGroup", data, None).await?;
        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64()) {
            return Ok(OK_CODES.contains(&(ret as u32)));
        }
        Ok(false)
    }

    async fn modify_group(
        &self,
        name: &str,
        newname: Option<&str>,
        comment: Option<&str>,
        auth: Option<Vec<Value>>,
    ) -> Result<bool> {
        let groups = self.get_groups().await?;
        let group = groups
            .iter()
            .find(|g| g.get("Name").and_then(|n| n.as_str()) == Some(name))
            .ok_or_else(|| {
                crate::error::DVRIPError::Unknown(format!("Group '{}' not found", name))
            })?;

        let auth_list = auth.unwrap_or_else(|| {
            group
                .get("AuthorityList")
                .and_then(|a| a.as_array())
                .cloned()
                .unwrap_or_default()
        });

        let data = json!({
            "Group": {
                "AuthorityList": auth_list,
                "Memo": comment.or_else(|| group.get("Memo").and_then(|m| m.as_str())).unwrap_or_default(),
                "Name": newname.unwrap_or(name),
            },
            "GroupName": name,
        });

        let reply = self.set_command("ModifyGroup", data, None).await?;
        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64()) {
            return Ok(OK_CODES.contains(&(ret as u32)));
        }
        Ok(false)
    }

    async fn delete_group(&self, name: &str) -> Result<bool> {
        let session = self.session_id();
        let data = json!({
            "Name": name,
            "SessionID": format!("0x{:08X}", session),
        });

        let reply = self.set_command("DelGroup", data, None).await?;
        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64()) {
            return Ok(OK_CODES.contains(&(ret as u32)));
        }
        Ok(false)
    }

    async fn get_users(&self) -> Result<Vec<Value>> {
        let data = self.get_command("Users", None).await?;
        if let Some(users) = data.get("Users").and_then(|v| v.as_array()) {
            return Ok(users.clone());
        }
        Ok(vec![])
    }

    async fn add_user(
        &self,
        name: &str,
        password: &str,
        comment: &str,
        group: &str,
        auth: Option<Vec<Value>>,
        sharable: bool,
    ) -> Result<bool> {
        let groups = self.get_groups().await?;
        let group_data = groups
            .iter()
            .find(|g| g.get("Name").and_then(|n| n.as_str()) == Some(group))
            .ok_or_else(|| {
                crate::error::DVRIPError::Unknown(format!("Group '{}' not found", group))
            })?;

        let auth_list = auth.unwrap_or_else(|| {
            group_data
                .get("AuthorityList")
                .and_then(|a| a.as_array())
                .cloned()
                .unwrap_or_default()
        });

        let data = json!({
            "User": {
                "AuthorityList": auth_list,
                "Group": group,
                "Memo": comment,
                "Name": name,
                "Password": sofia_hash(password),
                "Reserved": false,
                "Sharable": sharable,
            }
        });

        let reply = self.set_command("User", data, None).await?;
        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64()) {
            return Ok(OK_CODES.contains(&(ret as u32)));
        }
        Ok(false)
    }

    async fn modify_user(
        &self,
        name: &str,
        newname: Option<&str>,
        comment: Option<&str>,
        group: Option<&str>,
        auth: Option<Vec<Value>>,
        sharable: Option<bool>,
    ) -> Result<bool> {
        let users = self.get_users().await?;
        let user = users
            .iter()
            .find(|u| u.get("Name").and_then(|n| n.as_str()) == Some(name))
            .ok_or_else(|| {
                crate::error::DVRIPError::Unknown(format!("User '{}' not found", name))
            })?;

        let mut auth_list = user
            .get("AuthorityList")
            .and_then(|a| a.as_array())
            .cloned()
            .unwrap_or_default();

        if let Some(group_name) = group {
            let groups = self.get_groups().await?;
            if let Some(group_data) = groups
                .iter()
                .find(|g| g.get("Name").and_then(|n| n.as_str()) == Some(group_name))
            {
                auth_list = group_data
                    .get("AuthorityList")
                    .and_then(|a| a.as_array())
                    .cloned()
                    .unwrap_or_default();
            }
        }

        let data = json!({
            "User": {
                "AuthorityList": auth.unwrap_or(auth_list),
                "Group": group.or_else(|| user.get("Group").and_then(|g| g.as_str())).unwrap_or(""),
                "Memo": comment.or_else(|| user.get("Memo").and_then(|m| m.as_str())).unwrap_or_default(),
                "Name": newname.unwrap_or(name),
                "Password": "",
                "Reserved": user.get("Reserved").and_then(|r| r.as_bool()).unwrap_or(false),
                "Sharable": sharable.or_else(|| user.get("Sharable").and_then(|s| s.as_bool())).unwrap_or(false),
            },
            "UserName": name,
        });

        let reply = self.set_command("ModifyUser", data, None).await?;
        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64()) {
            return Ok(OK_CODES.contains(&(ret as u32)));
        }
        Ok(false)
    }

    async fn delete_user(&self, name: &str) -> Result<bool> {
        let session = self.session_id();
        let data = json!({
            "Name": name,
            "SessionID": format!("0x{:08X}", session),
        });

        let reply = self.set_command("DelUser", data, None).await?;
        if let Some(ret) = reply.get("Ret").and_then(|r| r.as_u64()) {
            return Ok(OK_CODES.contains(&(ret as u32)));
        }
        Ok(false)
    }
}
