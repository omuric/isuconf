#!/bin/sh
# Called as: sh -c "$script" isuconf <destination> <expected_size>
# Configuration bytes arrive on stdin; arguments are escaped by openssh.
set -eu

destination=${1:?Missing destination}
expected_size=${2:?Missing expected size}

# Keep the upload private until all bytes have arrived.
umask 077
staging_dir=$(mktemp -d)
trap 'rm -rf -- "$staging_dir"' EXIT
trap 'exit 1' HUP INT TERM
staged_file="$staging_dir/content"

cat > "$staged_file"
received_size=$(wc -c < "$staged_file")
if [ "$received_size" -ne "$expected_size" ]; then
    printf 'Incomplete configuration upload: expected %s bytes, received %s\n' \
        "$expected_size" "$received_size" >&2
    exit 1
fi

if [ -e "$destination" ]; then
    # Copy in place to preserve permissions, symlinks, and mounted config files.
    cp -- "$staged_file" "$destination"
else
    # New settings must be readable by the service that consumes them.
    install -m 0644 -- "$staged_file" "$destination"
fi
