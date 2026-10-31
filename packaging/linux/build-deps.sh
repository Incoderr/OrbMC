#!/usr/bin/env bash
# Libraries GPUI links against on Linux (X11, Wayland, Vulkan, fonts).
# Debian/Ubuntu; CI runs it on ubuntu-22.04 so the binary works on glibc 2.35+.
set -euo pipefail

sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  pkg-config \
  libfontconfig-dev \
  libfreetype-dev \
  libwayland-dev \
  libx11-xcb-dev \
  libxcb1-dev \
  libxkbcommon-dev \
  libxkbcommon-x11-dev \
  libvulkan-dev \
  libzstd-dev \
  libssl-dev \
  rpm
