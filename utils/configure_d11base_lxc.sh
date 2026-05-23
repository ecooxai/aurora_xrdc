#!/usr/bin/env bash
set -euo pipefail

INSTANCE="${1:-d11base}"
BRIDGE="${LXC_BRIDGE:-lxdbr0}"
TMP_DEVICE_NAMES=()
NIC_DEVICE_NAMES=()

if ! command -v lxc >/dev/null 2>&1; then
  echo "lxc command not found. Run this on the LXD/Incus host." >&2
  exit 1
fi

if ! lxc info "${INSTANCE}" >/dev/null 2>&1; then
  echo "LXC instance '${INSTANCE}' was not found." >&2
  exit 1
fi

collect_instance_devices() {
  local section="$1"
  lxc config device show "${INSTANCE}" | awk -v wanted_section="${section}" '
    function flush_device() {
      if (wanted_section == "tmp" && type == "disk" && path == "/tmp") {
        print name
      }
      if (wanted_section == "nic" && type == "nic" && (nictype != "bridged" || network == "" || network == "host")) {
        print name
      }
    }
    /^[^[:space:]].*:$/ {
      flush_device()
      name=$0
      sub(/:$/, "", name)
      type=""
      path=""
      nictype=""
      network=""
      next
    }
    /^[[:space:]]+type:[[:space:]]*/ {
      type=$2
      next
    }
    /^[[:space:]]+path:[[:space:]]*/ {
      path=$2
      next
    }
    /^[[:space:]]+nictype:[[:space:]]*/ {
      nictype=$2
      next
    }
    /^[[:space:]]+network:[[:space:]]*/ {
      network=$2
      next
    }
    END { flush_device() }
  '
}

mapfile -t TMP_DEVICE_NAMES < <(collect_instance_devices tmp)
for device in "${TMP_DEVICE_NAMES[@]}"; do
  if [[ -n "${device}" ]]; then
    echo "[lxc] removing /tmp disk device '${device}' from ${INSTANCE}"
    lxc config device remove "${INSTANCE}" "${device}"
  fi
done

mapfile -t NIC_DEVICE_NAMES < <(collect_instance_devices nic)
for device in "${NIC_DEVICE_NAMES[@]}"; do
  if [[ -n "${device}" ]]; then
    echo "[lxc] removing host/non-bridge NIC device '${device}' from ${INSTANCE}"
    lxc config device remove "${INSTANCE}" "${device}"
  fi
done

if ! lxc config device show "${INSTANCE}" | awk '
  function flush_device() {
    if (type == "nic" && network != "" && network != "host") {
      found=1
    }
  }
  /^[^[:space:]].*:$/ { flush_device(); name=$0; sub(/:$/, "", name); type=""; network="" }
  /^[[:space:]]+type:[[:space:]]*/ { type=$2 }
  /^[[:space:]]+network:[[:space:]]*/ { network=$2 }
  END { flush_device(); exit(found ? 0 : 1) }
'; then
  echo "[lxc] adding NAT bridge NIC eth0 on ${BRIDGE}"
  lxc config device add "${INSTANCE}" eth0 nic name=eth0 network="${BRIDGE}"
fi

echo "[lxc] enabling guest network services in ${INSTANCE}"
lxc exec "${INSTANCE}" -- bash -lc '
  set -euo pipefail

  if command -v systemctl >/dev/null 2>&1; then
    systemctl unmask networking.service systemd-networkd.service NetworkManager.service systemd-resolved.service >/dev/null 2>&1 || true

    if systemctl list-unit-files networking.service >/dev/null 2>&1; then
      systemctl enable networking.service >/dev/null 2>&1 || true
      systemctl restart networking.service >/dev/null 2>&1 || true
    fi

    if systemctl list-unit-files systemd-networkd.service >/dev/null 2>&1; then
      systemctl enable systemd-networkd.service >/dev/null 2>&1 || true
      systemctl restart systemd-networkd.service >/dev/null 2>&1 || true
    fi

    if systemctl list-unit-files systemd-resolved.service >/dev/null 2>&1; then
      systemctl enable systemd-resolved.service >/dev/null 2>&1 || true
      systemctl restart systemd-resolved.service >/dev/null 2>&1 || true
    fi

    if systemctl list-unit-files NetworkManager.service >/dev/null 2>&1; then
      systemctl enable NetworkManager.service >/dev/null 2>&1 || true
      systemctl restart NetworkManager.service >/dev/null 2>&1 || true
    fi
  fi

  if [[ -d /etc/network && ! -s /etc/network/interfaces ]]; then
    cat >/etc/network/interfaces <<EOF
auto lo
iface lo inet loopback

auto eth0
iface eth0 inet dhcp
EOF
  fi

  if [[ -d /etc/systemd/network && ! -e /etc/systemd/network/10-eth0.network ]]; then
    cat >/etc/systemd/network/10-eth0.network <<EOF
[Match]
Name=eth0

[Network]
DHCP=yes
EOF
  fi
'

echo "[lxc] ${INSTANCE} now uses guest /tmp and bridge networking via ${BRIDGE}."
