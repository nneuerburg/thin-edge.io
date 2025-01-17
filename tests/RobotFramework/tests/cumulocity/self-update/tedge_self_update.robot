*** Settings ***
Resource    ../../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO
Library    DateTime

Test Tags    theme:c8y    theme:installation
Test Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***

Update tedge version from previous using Cumulocity
    [Tags]    test:retry(1)    workaround

    ${PREV_VERSION}=    Set Variable    0.8.1
    # Install base version
    Execute Command    curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s ${PREV_VERSION}

    # Disable service (as it was enabled by default in 0.8.1)
    Execute Command    systemctl stop tedge-mapper-az && systemctl disable tedge-mapper-az

    # Register device (using already installed version)
    Execute Command    cmd=test -f ./bootstrap.sh && env DEVICE_ID=${DEVICE_SN} ./bootstrap.sh --no-install --no-secure || true
    Device Should Exist                      ${DEVICE_SN}

    Restart Service    tedge-mapper-c8y    # WORKAROUND: #1731 Restart service to avoid suspected race condition causing software list message to be lost

    # Note: Software type is reported as a part of version in thin-edge 0.8.1
    Device Should Have Installed Software    tedge,${PREV_VERSION}::apt    tedge_mapper,${PREV_VERSION}::apt    tedge_agent,${PREV_VERSION}::apt    tedge_watchdog,${PREV_VERSION}::apt    c8y_configuration_plugin,${PREV_VERSION}::apt    c8y_log_plugin,${PREV_VERSION}::apt    tedge_apt_plugin,${PREV_VERSION}::apt

    # Install desired version
    Create Local Repository
    ${OPERATION}=    Install Software    tedge,${NEW_VERSION}    tedge-mapper,${NEW_VERSION}    tedge-agent,${NEW_VERSION}    tedge-watchdog,${NEW_VERSION}    tedge-apt-plugin,${NEW_VERSION}
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=180

    # Software list reported by the former agent, which is still running
    # but formatted with by the c8y-mapper, which has just been installed
    Device Should Have Installed Software
    ...    {"name": "tedge", "type": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-mapper", "type": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-agent", "type": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-watchdog", "type": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-apt-plugin", "type": "apt", "version": "${NEW_VERSION_ESCAPED}"}

    # Restart tedge-agent from Cumulocity
    ${operation}=    Cumulocity.Restart Device
    Operation Should Be SUCCESSFUL    ${operation}    timeout=180

    # Software list reported by the new agent
    Device Should Have Installed Software
    ...    {"name": "tedge", "type": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-mapper", "type": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-agent", "type": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-watchdog", "type": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-apt-plugin", "type": "apt", "version": "${NEW_VERSION_ESCAPED}"}

    # Check if services are still stopped and disabled
    ${OUTPUT}    Execute Command    systemctl is-active tedge-mapper-az || exit 1    exp_exit_code=1    strip=True
    Should Be Equal    ${OUTPUT}    inactive    msg=Service should still be stopped
    ${OUTPUT}    Execute Command    systemctl is-enabled tedge-mapper-az || exit 1    exp_exit_code=1    strip=True
    Should Be Equal    ${OUTPUT}    disabled    msg=Service should still be disabled

    # Check that the mapper is reacting to operations after the upgrade
    # Notes:
    # * Bug as seen in the past: https://github.com/thin-edge/thin-edge.io/issues/2545
    # * PR to switch to using devicecontrol c8y topic: https://github.com/thin-edge/thin-edge.io/issues/1718
    ${operation}=    Cumulocity.Get Configuration    tedge-configuration-plugin
    Operation Should Be SUCCESSFUL    ${operation}

Refreshes mosquitto bridge configuration
    ${PREV_VERSION}=    Set Variable    0.10.0
    # Install base version
    Execute Command    curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s ${PREV_VERSION}

    # Register device (using already installed version)
    Execute Command    cmd=test -f ./bootstrap.sh && env DEVICE_ID=${DEVICE_SN} ./bootstrap.sh --no-install --no-secure || true
    Device Should Exist                      ${DEVICE_SN}

    # get bridge modification time
    ${before_upgrade_time}=    Execute Command      stat /etc/tedge/mosquitto-conf/c8y-bridge.conf -c %Y    strip=True

    # Install newer version
    Create Local Repository
    ${OPERATION}=    Install Software    tedge,${NEW_VERSION}    tedge-mapper,${NEW_VERSION}    tedge-agent,${NEW_VERSION}    tedge-watchdog,${NEW_VERSION}    tedge-apt-plugin,${NEW_VERSION}
    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=180

    # TODO: check that this new configuration is actually used by mosquitto
    ${c8y_bridge_mod_time}=    Execute Command      stat /etc/tedge/mosquitto-conf/c8y-bridge.conf -c %Y    strip=True
    Should Not Be Equal    ${c8y_bridge_mod_time}    ${before_upgrade_time}

    # Mosquitto should be restarted with new bridge
    Execute Command    cmd=sh -c '[ $(journalctl -u mosquitto | grep -c "Loading config file /etc/tedge/mosquitto-conf/c8y-bridge.conf") = 2 ]'

Update tedge version from base to current using Cumulocity
    ${arch}=    Get Debian Architecture
    Execute Command    mkdir -p /setup/base-version
    # These base-version packages are built using the ci/build_scripts/build.sh script for all 4 supported architectures
    # by overriding the version to a lower number using the GIT_SEMVER env variable
    Transfer To Device    ${CURDIR}/base-version/*${arch}.deb    /setup/base-version/

    # Install base version with self-update capability
    Install Packages    /setup/base-version
    Execute Command    cd /setup && test -f ./bootstrap.sh && ./bootstrap.sh --no-install --no-secure
    Device Should Exist                      ${DEVICE_SN}
    ${pid_before}=    Execute Command    pgrep tedge-agent    strip=${True}

    # Upgrade to current version
    Create Local Repository
    ${OPERATION}=    Install Software    tedge,${NEW_VERSION}    tedge-mapper,${NEW_VERSION}    tedge-agent,${NEW_VERSION}    tedge-watchdog,${NEW_VERSION}    tedge-apt-plugin,${NEW_VERSION}

    Operation Should Be SUCCESSFUL    ${OPERATION}    timeout=300
    Device Should Have Installed Software
    ...    {"name": "tedge", "type": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-mapper", "type": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-agent", "type": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-watchdog", "type": "apt", "version": "${NEW_VERSION_ESCAPED}"}
    ...    {"name": "tedge-apt-plugin", "type": "apt", "version": "${NEW_VERSION_ESCAPED}"}

    ${pid_after}=    Execute Command    pgrep tedge-agent    strip=${True}
    Should Not Be Equal    ${pid_before}    ${pid_after}

*** Keywords ***

Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=True
    Set Suite Variable    $DEVICE_SN

    # Cleanup
    Execute Command    rm -f /etc/tedge/tedge.toml /etc/tedge/system.toml && sudo dpkg --configure -a && apt-get purge -y "tedge*" "c8y*"
    Execute Command    cmd=rm -f /etc/apt/sources.list.d/thinedge*.list /etc/apt/sources.list.d/tedge*.list    # Remove any existing repositories (due to candidate bug in <= 0.8.1)

Create Local Repository
    [Arguments]    ${packages_dir}=/setup/packages
    # Create local apt repo
    Execute Command    apt-get install -y --no-install-recommends dpkg-dev
    Execute Command    mkdir -p /opt/repository/local && find ${packages_dir} -type f -name "*.deb" -exec cp {} /opt/repository/local \\;
    ${NEW_VERSION}=    Execute Command    find ${packages_dir} -type f -name "tedge-mapper_*.deb" | sort -Vr | head -n1 | cut -d'_' -f 2    strip=True
    Set Suite Variable    $NEW_VERSION
    ${NEW_VERSION_ESCAPED}=    Escape Pattern    ${NEW_VERSION}    is_json=${True}
    Set Suite Variable    $NEW_VERSION_ESCAPED
    Execute Command    cd /opt/repository/local && dpkg-scanpackages -m . > Packages
    Execute Command    cmd=echo 'deb [trusted=yes] file:/opt/repository/local /' > /etc/apt/sources.list.d/tedge-local.list

Install Packages
    [Arguments]    ${packages_dir}=/setup/packages

    Execute Command    apt-get update && apt-get install -y --no-install-recommends mosquitto
    Execute Command    cd ${packages_dir} && dpkg -i tedge_*.deb && dpkg -i tedge*mapper*.deb && dpkg -i tedge*agent*.deb && dpkg -i tedge*watchdog*.deb && dpkg -i tedge*apt*plugin*.deb
