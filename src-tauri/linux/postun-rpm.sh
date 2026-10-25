#!/bin/sh
# Package removal cleanup for the .rpm (%postun). Removes ONLY the app-managed engine
# folder in each user's data directory; never history, settings or user downloads.
#
# $1 is the number of package instances left: 0 on final removal, 1+ on upgrade.
if [ "$1" = "0" ]; then
  for home in /home/*; do
    dir="$home/.local/share/strs.pencil.downloadmanager/engine"
    if [ -d "$dir" ] && [ ! -L "$dir" ]; then
      rm -rf -- "$dir"
    fi
  done
fi
exit 0
