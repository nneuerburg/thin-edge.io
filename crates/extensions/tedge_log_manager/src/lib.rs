mod actor;
mod config;
mod error;

#[cfg(test)]
mod tests;

pub use actor::*;
pub use config::*;
use log_manager::LogPluginConfig;
use std::path::PathBuf;
use tedge_actors::adapt;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::LoggingSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::NoMessage;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceProvider;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::*;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::create_file_with_defaults;
use tedge_utils::file::move_file;
use tedge_utils::file::FileError;
use tedge_utils::file::PermissionEntry;

/// This is an actor builder.
pub struct LogManagerBuilder {
    config: LogManagerConfig,
    plugin_config: LogPluginConfig,
    box_builder: SimpleMessageBoxBuilder<LogInput, NoMessage>,
    mqtt_publisher: DynSender<MqttMessage>,
    upload_sender: DynSender<LogUploadRequest>,
}

impl LogManagerBuilder {
    pub async fn try_new(
        config: LogManagerConfig,
        mqtt: &mut impl ServiceProvider<MqttMessage, MqttMessage, TopicFilter>,
        fs_notify: &mut impl MessageSource<FsWatchEvent, PathBuf>,
        uploader_actor: &mut impl ServiceProvider<LogUploadRequest, LogUploadResult, NoConfig>,
    ) -> Result<Self, FileError> {
        Self::init(&config).await?;
        let plugin_config = LogPluginConfig::new(&config.plugin_config_path);

        let box_builder = SimpleMessageBoxBuilder::new("Log Manager", 16);
        let mqtt_publisher = mqtt.connect_consumer(
            Self::subscriptions(&config),
            adapt(&box_builder.get_sender()),
        );
        fs_notify.register_peer(
            LogManagerBuilder::watched_directory(&config),
            adapt(&box_builder.get_sender()),
        );

        let upload_sender =
            uploader_actor.connect_consumer(NoConfig, adapt(&box_builder.get_sender()));

        Ok(Self {
            config,
            plugin_config,
            box_builder,
            mqtt_publisher,
            upload_sender,
        })
    }

    pub async fn init(config: &LogManagerConfig) -> Result<(), FileError> {
        if config.plugin_config_path.exists() {
            return Ok(());
        }

        // creating plugin config parent dir
        create_directory_with_defaults(&config.plugin_config_dir)?;

        let legacy_plugin_config = config.config_dir.join("c8y").join("c8y-log-plugin.toml");
        if legacy_plugin_config.exists() {
            move_file(
                legacy_plugin_config,
                &config.plugin_config_path,
                PermissionEntry::default(),
            )
            .await?;
            return Ok(());
        }

        // creating tedge-log-plugin.toml
        let example_config = r#"# Add the list of log files that should be managed
files = [
#    { type = "mosquitto", path = '/var/log/mosquitto/mosquitto.log' },
#    { type = "software-management", path = '/var/log/tedge/agent/software-*' },
#    { type = "c8y_CustomOperation", path = '/var/log/tedge/agent/c8y_CustomOperation/*' }
]"#;
        create_file_with_defaults(&config.plugin_config_path, Some(example_config))?;

        Ok(())
    }

    /// List of MQTT topic filters the log actor has to subscribe to
    fn subscriptions(config: &LogManagerConfig) -> TopicFilter {
        config.logfile_request_topic.clone()
    }

    /// Directory watched by the log actors for configuration changes
    fn watched_directory(config: &LogManagerConfig) -> PathBuf {
        config.plugin_config_dir.clone()
    }
}

impl RuntimeRequestSink for LogManagerBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

impl Builder<LogManagerActor> for LogManagerBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<LogManagerActor, Self::Error> {
        let mqtt_publisher = LoggingSender::new("Tedge-Log-Manager".into(), self.mqtt_publisher);
        let message_box = self.box_builder.build();

        Ok(LogManagerActor::new(
            self.config,
            self.plugin_config,
            mqtt_publisher,
            message_box,
            self.upload_sender,
        ))
    }
}
