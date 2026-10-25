#!/bin/sh
# Package removal cleanup for the .deb (postrm). Removes ONLY the app-managed engine
# folder that the app may have downloaded into each user's data directory. It never
# touches history, settings or anything the user downloaded (use the in-app
# "Delete all app data" action for that).
#
# $1 is "remove", "purge", "upgrade", ... for a .deb. Upgrades must not remove anything.
case "$1" in
  remove|purge)
    for home in /home/*; do
      dir="$home/.local/share/strs.pencil.downloadmanager/engine"
      if [ -d "$dir" ] && [ ! -L "$dir" ]; then
        rm -rf -- "$dir"
      fi
    done
    ;;
esac
exit 0
