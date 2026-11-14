#!/usr/bin/env bash
# Installs a built bundle as a user would and checks it: the runtime comes
# from Flathub through the bundle's runtime repository, the app reports the
# release's version, and its metadata and desktop entry pass validation.
#
#   packaging/flatpak/check.sh Getman-linux-x86_64.flatpak v0.3.4
set -euo pipefail
bundle="$1"
tag="${2:-}"
app=io.github.fzn0x.getman

# A runner has no Flathub yet, which a person's system usually has.
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install --user --noninteractive "$bundle"

version="$(flatpak run --command=getman "$app" --version)"
echo "$version"
if [ -n "$tag" ] && [[ "$version" != *"${tag#v}"* ]]; then
  echo "::error::getman reports '$version', not ${tag#v}"
  exit 1
fi

files="$(flatpak info --user --show-location "$app")/files/share"
desktop-file-validate "$files/applications/$app.desktop"
appstreamcli validate --no-net --explain "$files/metainfo/$app.metainfo.xml"
