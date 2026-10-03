use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::AccountService;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceivingDefaults {
    pub receiving_policy: String,
    pub auto_download_friends: bool,
    pub revision: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveReceiving {
    pub can_send: bool,
    pub auto_download: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContactIdentity {
    pub id: u64,
    pub username: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    pub id: u64,
    pub username: String,
    pub name: String,
    pub status: String,
    pub can_send: Option<bool>,
    pub auto_download: Option<bool>,
    pub effective: EffectiveReceiving,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Contacts {
    pub settings: ReceivingDefaults,
    pub contacts: Vec<Contact>,
    pub blocked: Vec<ContactIdentity>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct InboxSyncItem {
    pub id: String,
    pub ciphertext_bytes: u64,
    pub item_count: u64,
    pub expires_at: String,
    #[serde(rename = "autoDownload")]
    pub auto_download: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct InboxSync {
    pub revision: u64,
    pub next: Option<String>,
    pub transfers: Vec<InboxSyncItem>,
}

pub fn contact_username(value: &str) -> Result<&str> {
    let value = value.strip_prefix('@').unwrap_or(value);
    if !(3..=24).contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        bail!("use an exact local-instance username, such as @alice");
    }
    Ok(value)
}

impl AccountService {
    pub fn contacts(&self) -> Result<Contacts> {
        self.get("api/native/v1/contacts")
    }

    pub fn contact_lookup(&self, username: &str) -> Result<ContactIdentity> {
        self.get(&format!(
            "api/native/v1/contacts/{}",
            contact_username(username)?
        ))
    }

    pub fn contact_action(
        &self,
        username: &str,
        action: &str,
        can_send: Option<bool>,
        auto_download: Option<bool>,
    ) -> Result<Contacts> {
        if !matches!(
            action,
            "request"
                | "accept"
                | "decline"
                | "cancel"
                | "remove"
                | "block"
                | "unblock"
                | "preferences"
        ) {
            bail!("invalid contact action");
        }
        self.post(&format!("api/native/v1/contacts/{}", contact_username(username)?), serde_json::json!({"action": action, "canSend": can_send, "autoDownload": auto_download}), 200)
    }

    pub fn set_receiving_defaults(
        &self,
        policy: &str,
        auto_download: bool,
    ) -> Result<ReceivingDefaults> {
        if !matches!(policy, "anyone" | "authenticated" | "friends" | "nobody") {
            bail!("receiving policy must be anyone, authenticated, friends, or nobody");
        }
        let response = self.http.patch(super::client::url(&self.instance, "api/native/v1/account/receiving")?)
            .header("Sec-Fetch-Site", "same-origin").header("Accept", "application/json")
            .json(&serde_json::json!({"receivingPolicy": policy, "autoDownloadFriends": auto_download})).send()?;
        if !response.status().is_success() {
            bail!("receiving settings returned {}", response.status());
        }
        Ok(response
            .json::<super::account::Api<ReceivingDefaults>>()?
            .data)
    }

    pub fn inbox_sync(&self, after: Option<&str>) -> Result<InboxSync> {
        let suffix = match after {
            Some(id) => format!("?after={id}"),
            None => String::new(),
        };
        self.get(&format!("api/native/v1/inbox/sync{suffix}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contact_identities_cannot_escape_the_instance_route() {
        assert_eq!(contact_username("@alice_1").unwrap(), "alice_1");
        for value in ["alice@remote", "../alice", "Alice", "a/b", "alice?x", "al"] {
            assert!(contact_username(value).is_err());
        }
    }
}
