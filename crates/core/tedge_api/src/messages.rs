use crate::error::SoftwareError;
use crate::mqtt_topics::Channel;
use crate::mqtt_topics::EntityTopicId;
use crate::mqtt_topics::MqttSchema;
use crate::mqtt_topics::OperationType;
use crate::software::*;
use download::DownloadInfo;
use mqtt_channel::Message;
use mqtt_channel::QoS;
use mqtt_channel::Topic;
use nanoid::nanoid;
use serde::Deserialize;
use serde::Serialize;
use time::OffsetDateTime;

// TODO deprecate these consts
pub const SOFTWARE_LIST_REQUEST_TOPIC: &str = "tedge/commands/req/software/list";
pub const SOFTWARE_LIST_RESPONSE_TOPIC: &str = "tedge/commands/res/software/list";
pub const SOFTWARE_UPDATE_REQUEST_TOPIC: &str = "tedge/commands/req/software/update";
pub const SOFTWARE_UPDATE_RESPONSE_TOPIC: &str = "tedge/commands/res/software/update";

/// All the messages are serialized using json.
pub trait Jsonify<'a>
where
    Self: Deserialize<'a> + Serialize + Sized,
{
    fn from_json(json_str: &'a str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    fn from_slice(bytes: &'a [u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap() // all thin-edge data can be serialized to json
    }

    fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap() // all thin-edge data can be serialized to json
    }
}

/// Message payload definition for SoftwareList request.
#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareListRequest {
    pub id: String,
}

impl<'a> Jsonify<'a> for SoftwareListRequest {}

impl Default for SoftwareListRequest {
    fn default() -> SoftwareListRequest {
        let id = nanoid!();
        SoftwareListRequest { id }
    }
}

impl SoftwareListRequest {
    pub fn new_with_id(id: &str) -> SoftwareListRequest {
        SoftwareListRequest { id: id.to_string() }
    }

    pub fn topic() -> Topic {
        Topic::new_unchecked(SOFTWARE_LIST_REQUEST_TOPIC)
    }
}

/// Message payload definition for SoftwareUpdate request.
#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareUpdateRequest {
    pub id: String,
    pub update_list: Vec<SoftwareRequestResponseSoftwareList>,
}

impl<'a> Jsonify<'a> for SoftwareUpdateRequest {}

impl Default for SoftwareUpdateRequest {
    fn default() -> SoftwareUpdateRequest {
        let id = nanoid!();
        SoftwareUpdateRequest {
            id,
            update_list: vec![],
        }
    }
}

impl SoftwareUpdateRequest {
    pub fn new_with_id(id: &str) -> SoftwareUpdateRequest {
        SoftwareUpdateRequest {
            id: id.to_string(),
            update_list: vec![],
        }
    }

    pub fn topic() -> Topic {
        Topic::new_unchecked(SOFTWARE_UPDATE_REQUEST_TOPIC)
    }

    pub fn add_update(&mut self, mut update: SoftwareModuleUpdate) {
        update.normalize();
        let plugin_type = update
            .module()
            .module_type
            .clone()
            .unwrap_or_else(SoftwareModule::default_type);

        if let Some(list) = self
            .update_list
            .iter_mut()
            .find(|list| list.plugin_type == plugin_type)
        {
            list.modules.push(update.into());
        } else {
            self.update_list.push(SoftwareRequestResponseSoftwareList {
                plugin_type,
                modules: vec![update.into()],
            });
        }
    }

    pub fn add_updates(&mut self, plugin_type: &str, updates: Vec<SoftwareModuleUpdate>) {
        self.update_list.push(SoftwareRequestResponseSoftwareList {
            plugin_type: plugin_type.to_string(),
            modules: updates
                .into_iter()
                .map(|update| update.into())
                .collect::<Vec<SoftwareModuleItem>>(),
        })
    }

    pub fn modules_types(&self) -> Vec<SoftwareType> {
        let mut modules_types = vec![];

        for updates_per_type in self.update_list.iter() {
            modules_types.push(updates_per_type.plugin_type.clone())
        }

        modules_types
    }

    pub fn updates_for(&self, module_type: &str) -> Vec<SoftwareModuleUpdate> {
        let mut updates = vec![];

        if let Some(items) = self
            .update_list
            .iter()
            .find(|&items| items.plugin_type == module_type)
        {
            for item in items.modules.iter() {
                let module = SoftwareModule {
                    module_type: Some(module_type.to_string()),
                    name: item.name.clone(),
                    version: item.version.clone(),
                    url: item.url.clone(),
                    file_path: None,
                };
                match item.action {
                    None => {}
                    Some(SoftwareModuleAction::Install) => {
                        updates.push(SoftwareModuleUpdate::install(module));
                    }
                    Some(SoftwareModuleAction::Remove) => {
                        updates.push(SoftwareModuleUpdate::remove(module));
                    }
                }
            }
        }

        updates
    }
}

/// Sub list of modules grouped by plugin type.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct SoftwareRequestResponseSoftwareList {
    #[serde(rename = "type")]
    pub plugin_type: SoftwareType,
    pub modules: Vec<SoftwareModuleItem>,
}

/// Possible statuses for result of Software operation.
#[derive(Debug, Deserialize, Serialize, PartialEq, Copy, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub enum OperationStatus {
    Successful,
    Failed,
    Executing,
}

/// Message payload definition for SoftwareList response.
#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SoftwareListResponse {
    #[serde(flatten)]
    pub response: SoftwareRequestResponse,
}

impl<'a> Jsonify<'a> for SoftwareListResponse {}

impl SoftwareListResponse {
    pub fn new(req: &SoftwareListRequest) -> SoftwareListResponse {
        SoftwareListResponse {
            response: SoftwareRequestResponse::new(&req.id, OperationStatus::Executing),
        }
    }

    pub fn topic() -> Topic {
        Topic::new_unchecked(SOFTWARE_LIST_RESPONSE_TOPIC)
    }

    pub fn add_modules(&mut self, plugin_type: &str, modules: Vec<SoftwareModule>) {
        self.response.add_modules(
            plugin_type.to_string(),
            modules
                .into_iter()
                .map(|module| module.into())
                .collect::<Vec<SoftwareModuleItem>>(),
        );
    }

    pub fn set_error(&mut self, reason: &str) {
        self.response.status = OperationStatus::Failed;
        self.response.reason = Some(reason.into());
    }

    pub fn id(&self) -> &str {
        &self.response.id
    }

    pub fn status(&self) -> OperationStatus {
        self.response.status
    }

    pub fn error(&self) -> Option<String> {
        self.response.reason.clone()
    }

    pub fn modules(&self) -> Vec<SoftwareModule> {
        self.response.modules()
    }
}

/// Message payload definition for SoftwareUpdate response.
#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct SoftwareUpdateResponse {
    #[serde(flatten)]
    pub response: SoftwareRequestResponse,
}

impl<'a> Jsonify<'a> for SoftwareUpdateResponse {}

impl SoftwareUpdateResponse {
    pub fn new(req: &SoftwareUpdateRequest) -> SoftwareUpdateResponse {
        SoftwareUpdateResponse {
            response: SoftwareRequestResponse::new(&req.id, OperationStatus::Executing),
        }
    }

    pub fn topic() -> Topic {
        Topic::new_unchecked(SOFTWARE_UPDATE_RESPONSE_TOPIC)
    }

    pub fn add_modules(&mut self, plugin_type: &str, modules: Vec<SoftwareModule>) {
        self.response.add_modules(
            plugin_type.to_string(),
            modules
                .into_iter()
                .map(|module| module.into())
                .collect::<Vec<SoftwareModuleItem>>(),
        );
    }

    pub fn add_errors(&mut self, plugin_type: &str, errors: Vec<SoftwareError>) {
        self.response.add_errors(
            plugin_type.to_string(),
            errors
                .into_iter()
                .filter_map(|module| module.into())
                .collect::<Vec<SoftwareModuleItem>>(),
        );
    }

    pub fn set_error(&mut self, reason: &str) {
        self.response.status = OperationStatus::Failed;
        self.response.reason = Some(reason.into());
    }

    pub fn id(&self) -> &str {
        &self.response.id
    }

    pub fn status(&self) -> OperationStatus {
        self.response.status
    }

    pub fn error(&self) -> Option<String> {
        self.response.reason.clone()
    }

    pub fn modules(&self) -> Vec<SoftwareModule> {
        self.response.modules()
    }
}

/// Variants represent Software Operations Supported actions.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SoftwareModuleAction {
    Install,
    Remove,
}

/// Software module payload definition.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct SoftwareModuleItem {
    pub name: SoftwareName,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<SoftwareVersion>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(flatten)]
    pub url: Option<DownloadInfo>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<SoftwareModuleAction>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Software Operation Response payload format.
#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareRequestResponse {
    pub id: String,
    pub status: OperationStatus,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_software_list: Option<Vec<SoftwareRequestResponseSoftwareList>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<SoftwareRequestResponseSoftwareList>,
}

impl<'a> Jsonify<'a> for SoftwareRequestResponse {}

impl SoftwareRequestResponse {
    pub fn new(id: &str, status: OperationStatus) -> Self {
        SoftwareRequestResponse {
            id: id.to_string(),
            status,
            current_software_list: None,
            reason: None,
            failures: vec![],
        }
    }

    pub fn add_modules(&mut self, plugin_type: SoftwareType, modules: Vec<SoftwareModuleItem>) {
        if self.failures.is_empty() {
            self.status = OperationStatus::Successful;
        }

        if self.current_software_list.is_none() {
            self.current_software_list = Some(vec![]);
        }

        if let Some(list) = self.current_software_list.as_mut() {
            list.push(SoftwareRequestResponseSoftwareList {
                plugin_type,
                modules,
            })
        }
    }

    pub fn add_errors(&mut self, plugin_type: SoftwareType, modules: Vec<SoftwareModuleItem>) {
        self.status = OperationStatus::Failed;

        self.failures.push(SoftwareRequestResponseSoftwareList {
            plugin_type,
            modules,
        })
    }

    pub fn modules(&self) -> Vec<SoftwareModule> {
        let mut modules = vec![];

        if let Some(list) = &self.current_software_list {
            for module_per_plugin in list.iter() {
                let module_type = &module_per_plugin.plugin_type;
                for module in module_per_plugin.modules.iter() {
                    modules.push(SoftwareModule {
                        module_type: Some(module_type.clone()),
                        name: module.name.clone(),
                        version: module.version.clone(),
                        url: module.url.clone(),
                        file_path: None,
                    });
                }
            }
        }

        modules
    }
}

impl From<SoftwareModule> for SoftwareModuleItem {
    fn from(module: SoftwareModule) -> Self {
        SoftwareModuleItem {
            name: module.name,
            version: module.version,
            url: module.url,
            action: None,
            reason: None,
        }
    }
}

impl From<SoftwareModuleUpdate> for SoftwareModuleItem {
    fn from(update: SoftwareModuleUpdate) -> Self {
        match update {
            SoftwareModuleUpdate::Install { module } => SoftwareModuleItem {
                name: module.name,
                version: module.version,
                url: module.url,
                action: Some(SoftwareModuleAction::Install),
                reason: None,
            },
            SoftwareModuleUpdate::Remove { module } => SoftwareModuleItem {
                name: module.name,
                version: module.version,
                url: module.url,
                action: Some(SoftwareModuleAction::Remove),
                reason: None,
            },
        }
    }
}

impl From<SoftwareError> for Option<SoftwareModuleItem> {
    fn from(error: SoftwareError) -> Self {
        match error {
            SoftwareError::Install { module, reason } => Some(SoftwareModuleItem {
                name: module.name,
                version: module.version,
                url: module.url,
                action: Some(SoftwareModuleAction::Install),
                reason: Some(reason),
            }),
            SoftwareError::Remove { module, reason } => Some(SoftwareModuleItem {
                name: module.name,
                version: module.version,
                url: module.url,
                action: Some(SoftwareModuleAction::Remove),
                reason: Some(reason),
            }),
            _ => None,
        }
    }
}

/// Command to restart a device
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RestartCommand {
    pub target: EntityTopicId,
    pub cmd_id: String,
    pub payload: RestartCommandPayload,
}

impl RestartCommand {
    /// Create a [RestartCommand] to restart a target device.
    ///
    /// - use a fresh cmd id
    /// - set the status to [CommandStatus::Init]
    pub fn new(target: EntityTopicId) -> Self {
        let cmd_id = nanoid!();
        let payload = RestartCommandPayload::default();
        RestartCommand {
            target,
            cmd_id,
            payload,
        }
    }

    /// A new command with a given cmd id
    pub fn with_id(self, cmd_id: String) -> Self {
        Self { cmd_id, ..self }
    }

    /// Return the RestartCommand received on a topic
    pub fn try_from(
        target: EntityTopicId,
        cmd_id: String,
        bytes: &[u8],
    ) -> Result<Option<Self>, serde_json::Error> {
        if bytes.is_empty() {
            Ok(None)
        } else {
            let payload = RestartCommandPayload::from_slice(bytes)?;
            Ok(Some(RestartCommand {
                target,
                cmd_id,
                payload,
            }))
        }
    }

    /// Return the current status of the command
    pub fn status(&self) -> CommandStatus {
        self.payload.status.clone()
    }

    /// Set the status of the command
    pub fn with_status(mut self, status: CommandStatus) -> Self {
        self.payload.status = status;
        self
    }

    /// Set the failure reason of the command
    pub fn with_error(mut self, reason: String) -> Self {
        self.payload.status = CommandStatus::Failed { reason };
        self
    }

    /// Return the MQTT topic identifier of the target
    fn topic_id(&self) -> &EntityTopicId {
        &self.target
    }

    /// Return the MQTT channel for this command
    fn channel(&self) -> Channel {
        Channel::Command {
            operation: OperationType::Restart,
            cmd_id: self.cmd_id.clone(),
        }
    }

    /// Return the MQTT topic for this command
    fn topic(&self, schema: &MqttSchema) -> Topic {
        schema.topic_for(self.topic_id(), &self.channel())
    }

    /// Return the MQTT message to register `restart` as a supported command on a given target device
    pub fn capability_message(schema: &MqttSchema, target: &EntityTopicId) -> Message {
        let meta_topic = schema.topic_for(
            target,
            &Channel::CommandMetadata {
                operation: OperationType::Restart,
            },
        );
        let payload = "{}";
        Message::new(&meta_topic, payload)
            .with_retain()
            .with_qos(QoS::AtLeastOnce)
    }

    /// Return the MQTT message for this command
    pub fn command_message(&self, schema: &MqttSchema) -> Message {
        let topic = self.topic(schema);
        let payload = self.payload.to_bytes();
        Message::new(&topic, payload)
            .with_qos(QoS::AtLeastOnce)
            .with_retain()
    }

    /// Return the MQTT message to clear this command
    pub fn clearing_message(&self, schema: &MqttSchema) -> Message {
        let topic = self.topic(schema);
        Message::new(&topic, vec![])
            .with_qos(QoS::AtLeastOnce)
            .with_retain()
    }
}

/// Command to restart a device
#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RestartCommandPayload {
    #[serde(flatten)]
    pub status: CommandStatus,
}

impl<'a> Jsonify<'a> for RestartCommandPayload {}

impl Default for RestartCommandPayload {
    fn default() -> Self {
        RestartCommandPayload {
            status: CommandStatus::Init,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum CommandStatus {
    Init,
    Executing,
    Successful,
    Failed { reason: String },
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LogMetadata {
    pub types: Vec<String>,
}

impl<'a> Jsonify<'a> for LogMetadata {}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LogUploadCmdPayload {
    #[serde(flatten)]
    pub status: CommandStatus,
    pub tedge_url: String,
    #[serde(rename = "type")]
    pub log_type: String,
    #[serde(with = "time::serde::rfc3339")]
    pub date_from: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub date_to: OffsetDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_text: Option<String>,
    pub lines: usize,
}

impl<'a> Jsonify<'a> for LogUploadCmdPayload {}

impl LogUploadCmdPayload {
    pub fn executing(&mut self) {
        self.status = CommandStatus::Executing;
    }

    pub fn successful(&mut self) {
        self.status = CommandStatus::Successful;
    }

    pub fn failed(&mut self, reason: impl Into<String>) {
        self.status = CommandStatus::Failed {
            reason: reason.into(),
        };
    }
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMetadata {
    pub types: Vec<String>,
}

impl<'a> Jsonify<'a> for ConfigMetadata {}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSnapshotCmdPayload {
    #[serde(flatten)]
    pub status: CommandStatus,
    pub tedge_url: String,
    #[serde(rename = "type")]
    pub config_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl<'a> Jsonify<'a> for ConfigSnapshotCmdPayload {}

impl ConfigSnapshotCmdPayload {
    pub fn executing(&mut self) {
        self.status = CommandStatus::Executing;
    }

    pub fn successful(&mut self, path: impl Into<String>) {
        self.status = CommandStatus::Successful;
        self.path = Some(path.into())
    }

    pub fn failed(&mut self, reason: impl Into<String>) {
        self.status = CommandStatus::Failed {
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConfigUpdateCmdPayload {
    #[serde(flatten)]
    pub status: CommandStatus,
    pub tedge_url: String,
    pub remote_url: String,
    #[serde(rename = "type")]
    pub config_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl<'a> Jsonify<'a> for ConfigUpdateCmdPayload {}

impl ConfigUpdateCmdPayload {
    pub fn executing(&mut self) {
        self.status = CommandStatus::Executing;
    }

    pub fn successful(&mut self, path: impl Into<String>) {
        self.status = CommandStatus::Successful;
        self.path = Some(path.into())
    }

    pub fn failed(&mut self, reason: impl Into<String>) {
        self.status = CommandStatus::Failed {
            reason: reason.into(),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_software_request_list() {
        let request = SoftwareListRequest {
            id: "1234".to_string(),
        };
        let expected_json = r#"{"id":"1234"}"#;

        let actual_json = request.to_json();

        assert_eq!(actual_json, expected_json);

        let de_request =
            SoftwareListRequest::from_json(actual_json.as_str()).expect("failed to deserialize");
        assert_eq!(request, de_request);
    }

    #[test]
    fn serde_software_request_update() {
        let debian_module1 = SoftwareModuleItem {
            name: "debian1".into(),
            version: Some("0.0.1".into()),
            action: Some(SoftwareModuleAction::Install),
            url: None,
            reason: None,
        };

        let debian_module2 = SoftwareModuleItem {
            name: "debian2".into(),
            version: Some("0.0.2".into()),
            action: Some(SoftwareModuleAction::Install),
            url: None,
            reason: None,
        };

        let debian_list = SoftwareRequestResponseSoftwareList {
            plugin_type: "debian".into(),
            modules: vec![debian_module1, debian_module2],
        };

        let docker_module1 = SoftwareModuleItem {
            name: "docker1".into(),
            version: Some("0.0.1".into()),
            action: Some(SoftwareModuleAction::Remove),
            url: Some("test.com".into()),
            reason: None,
        };

        let docker_list = SoftwareRequestResponseSoftwareList {
            plugin_type: "docker".into(),
            modules: vec![docker_module1],
        };

        let request = SoftwareUpdateRequest {
            id: "1234".to_string(),
            update_list: vec![debian_list, docker_list],
        };

        let expected_json = r#"{"id":"1234","updateList":[{"type":"debian","modules":[{"name":"debian1","version":"0.0.1","action":"install"},{"name":"debian2","version":"0.0.2","action":"install"}]},{"type":"docker","modules":[{"name":"docker1","version":"0.0.1","url":"test.com","action":"remove"}]}]}"#;

        let actual_json = request.to_json();
        assert_eq!(actual_json, expected_json);

        let parsed_request =
            SoftwareUpdateRequest::from_json(&actual_json).expect("Fail to parse the json request");
        assert_eq!(parsed_request, request);
    }

    #[test]
    fn serde_software_list_empty_successful() {
        let request = SoftwareRequestResponse {
            id: "1234".to_string(),
            status: OperationStatus::Successful,
            reason: None,
            current_software_list: Some(vec![]),
            failures: vec![],
        };

        let expected_json = r#"{"id":"1234","status":"successful","currentSoftwareList":[]}"#;

        let actual_json = request.to_json();
        assert_eq!(actual_json, expected_json);

        let parsed_request = SoftwareRequestResponse::from_json(&actual_json)
            .expect("Fail to parse the json request");
        assert_eq!(parsed_request, request);
    }

    #[test]
    fn serde_software_list_some_modules_successful() {
        let module1 = SoftwareModuleItem {
            name: "debian1".into(),
            version: Some("0.0.1".into()),
            action: None,
            url: None,
            reason: None,
        };

        let docker_module1 = SoftwareRequestResponseSoftwareList {
            plugin_type: "debian".into(),
            modules: vec![module1],
        };

        let request = SoftwareRequestResponse {
            id: "1234".to_string(),
            status: OperationStatus::Successful,
            reason: None,
            current_software_list: Some(vec![docker_module1]),
            failures: vec![],
        };

        let expected_json = r#"{"id":"1234","status":"successful","currentSoftwareList":[{"type":"debian","modules":[{"name":"debian1","version":"0.0.1"}]}]}"#;

        let actual_json = request.to_json();
        assert_eq!(actual_json, expected_json);

        let parsed_request = SoftwareRequestResponse::from_json(&actual_json)
            .expect("Fail to parse the json request");
        assert_eq!(parsed_request, request);
    }
}
