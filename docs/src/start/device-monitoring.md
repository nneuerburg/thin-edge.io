---
title: Monitoring
tags: [Getting Started, Monitoring, Collectd]
sidebar_position: 8
---

# Monitor your device with collectd

With thin-edge.io device monitoring, you can collect metrics from your device
and forward these device metrics to IoT platforms in the cloud.

Using these metrics, you can monitor the health of devices
and can proactively initiate actions in case the device seems to malfunction.
Additionally, the metrics can be used to help the customer troubleshoot when problems with the device are reported.

Thin-edge.io uses the open source component [`collectd`](https://collectd.org/) to collect the metrics from the device.
Thin-edge.io translates the collected metrics from their native format to the [thin-edge.io JSON](../understand/thin-edge-json.md) format
and then into the [cloud-vendor specific format](../understand/tedge-mapper.md).

Enabling monitoring on your device is a 3-steps process:
1. [Install `collectd`](#install-collectd),
2. [Configure `collectd`](#configure-collectd),
3. [Enable thin-edge.io monitoring](#enable-thin-edge-monitoring).

## Install collectd

Device monitoring is not enabled by default when you install thin edge.
You will have to install and configure [`collectd`](https://collectd.org/) first.

To install collectd, follow the [collectd installation process](https://collectd.org/download.shtml)
that is specific to your device. On a Debian or Ubuntu linux:

```sh title="Option 1: (Recommended)"
sudo apt-get install --no-install-recommends collectd-core mosquitto-clients
```

```sh title="Option 2"
sudo apt-get install --no-install-recommends collectd-core libmosquitto1
```

:::note
Either `mosquitto-clients` or `libmosquitto1` package is required as thin-edge.io makes use of the MQTT plugin of `collectd` which enables collectd to publish its metrics via the local MQTT broker where thin-edge.io is able to process the messages.
:::

## Configure collectd

### TLDR; Just want it running

Thin-edge.io provides a [basic `collectd` configuration](https://github.com/thin-edge/thin-edge.io/blob/main/configuration/contrib/collectd/collectd.conf)
that can be used to collect cpu, memory and disk metrics.

Simply copy that file to the main collectd configuration file and restart the daemon
(it might be good to keep a copy of the original configuration).

```sh
sudo cp /etc/collectd/collectd.conf /etc/collectd/collectd.conf.backup
sudo cp /etc/tedge/contrib/collectd/collectd.conf /etc/collectd/collectd.conf
sudo systemctl restart collectd
```

:::note
The `collectd.conf` file included with thin-edge.io is configured for conservative interval times, e.g. 10 mins to 1 hour depending on the metric. This is done so that the metrics don't consume unnecessary IoT resources both on the device and in the cloud. If you want to push the metrics more frequently then you will have to adjust the `Interval` settings either globally or on the individual plugins. Make sure you restart the collectd service after making any changes to the configuration.
:::

### Collectd.conf

Unless you opted for the [minimal test configuration provided with thin-edge](#tldr-just-want-it-running),
you will have to update the
[`collectd.conf` configuration file](https://collectd.org/documentation/manpages/collectd.conf.5.shtml)
(usually located at `/etc/collectd/collectd.conf`)

__Important notes__ You can enable or disable the collectd plugins of your choice, but with some notable exceptions:
1. __MQTT must be enabled__.
   * Thin-edge.io expects the collectd metrics to be published on the local MQTT bus.
     Hence, you must enable the [MQTT write plugin of collectd](https://collectd.org/documentation/manpages/collectd.conf.5.shtml#plugin_mqtt).
   * The MQTT plugin is available on most distribution of `collectd`, but this is not the case on MacOS using homebrew.
     If you are missing the MQTT plugin, please recompile `collectd` to include the MQTT plugin.
     See [https://github.com/collectd/collectd](https://github.com/collectd/collectd) for details.
   * Here is a config snippet to configure the MQTT write plugin:

     ```xml
      LoadPlugin mqtt

      <Plugin mqtt>
          <Publish "tedge">
              Host "localhost"
              Port 1883
              ClientId "tedge-collectd"
          </Publish>
      </Plugin>
     ```
2. __RRDTool and CSV might be disabled__
   * The risk with these plugins is to run out of disk space on a small device.
   * With thin-edge.io the metrics collected by `collectd` are forwarded to the cloud,
     hence it makes sense to [disable Local storage](https://github.com/collectd/collectd/issues/2668).
   * For that, simply comment out these two plugins:

      ```sh
      #LoadPlugin rrdtool
      #LoadPlugin csv
      ```
3. __Cherry-pick the collected metrics__
   * `Collectd` can collect a lot of detailed metrics,
      and it doesn't always make sense to forward all these data to the cloud.
   * Here is a config snippet that uses the `match_regex` plugin to select the metrics of interest,
     filtering out every metric emitted by the memory plugin other than the used metric":

      ```xml
      PreCacheChain "PreCache"

      LoadPlugin match_regex

      <Chain "PreCache">
          <Rule "memory_free_only">
              <Match "regex">
                  Plugin "memory"
              </Match>
              <Match "regex">
                  TypeInstance "used"
                  Invert true
              </Match>
              Target "stop"
          </Rule>
      </Chain>
      ```

## Enable thin-edge monitoring

To enable monitoring on your device, you have to launch the `tedge-mapper-collectd` daemon process.

```sh
sudo systemctl enable tedge-mapper-collectd
sudo systemctl start tedge-mapper-collectd
```

This process subscribes to the `collectd/#` topics to read the monitoring metrics published by collectd
and emits the translated measurements in thin-edge.io JSON format to the **measurements** topic.
You can inspect the collected and translated metrics, by subscribing to these topics:

The metrics collected by `collectd` are emitted to subtopics named after the collectd plugin and the metric name:

```sh te2mqtt formats=v1
tedge mqtt sub 'collectd/#'
```

```log title="Output"
[collectd/raspberrypi/cpu/percent-active] 1623076679.154:0.50125313283208
[collectd/raspberrypi/memory/percent-used] 1623076679.159:1.10760866126707
[collectd/raspberrypi/cpu/percent-active] 1623076680.154:0
[collectd/raspberrypi/df-root/percent_bytes-used] 1623076680.158:71.3109359741211
[collectd/raspberrypi/memory/percent-used] 1623076680.159:1.10760866126707
```

The `tedge-mapper-collectd` translates these collectd measurements into the [thin-edge.io JSON](../understand/thin-edge-json.md) format,
[grouping the measurements](../references/mappers/mqtt-topics.md#collectd-topics) emitted by each plugin:

```sh te2mqtt formats=v1
tedge mqtt sub 'te/+/+/+/+/m/+'
```

```log title="Output"
[te/device/main///m/] {"time":"2021-06-07T15:38:59.154895598+01:00","cpu":{"percent-active":0.50251256281407},"memory":{"percent-used":1.11893578135189}}
[te/device/main///m/] {"time":"2021-06-07T15:39:00.154967388+01:00","cpu":{"percent-active":0},"df-root":{"percent_bytes-used":71.3110656738281},"memory":{"percent-used":1.12107875001658}}
```

From there, if the device is actually connected to a cloud platform like Cumulocity,
these monitoring metrics will be forwarded to the cloud.

```sh te2mqtt formats=v1
tedge mqtt sub 'c8y/#'
```

```log title="Output"
[c8y/measurement/measurements/create] {"type": "ThinEdgeMeasurement","time":"2021-06-07T15:40:30.155037451+01:00","cpu":{"percent-active": {"value": 0.753768844221106}},"memory":{"percent-used": {"value": 1.16587699972141}},"df-root":{"percent_bytes-used": {"value": 71.3117904663086}}}
[c8y/measurement/measurements/create] {"type": "ThinEdgeMeasurement","time":"2021-06-07T15:40:31.154898577+01:00","cpu":{"percent-active": {"value": 0.5}},"memory":{"percent-used": {"value": 1.16608109197519}}}
```

If your device is not connected yet one of the following guides:
* [Connect my device to Cumulocity IoT](./connect-c8y.md)
* [Connect my device to Azure IoT](./connect-azure.md)
* [Connect my device to AWS IoT](./connect-aws.md)

## Troubleshooting

See here for [how to trouble shoot device monitoring?](../operate/troubleshooting/trouble_shooting_monitoring.md)
