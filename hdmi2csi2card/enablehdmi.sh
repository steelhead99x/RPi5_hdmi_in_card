#!/usr/bin/env bash
# Load a TC358743 EDID on a Raspberry Pi 5 and point the CSI-2 pipeline at it.
# Device numbers change from boot to boot, so this script reads the media graph
# instead of assuming /dev/video0 or /dev/v4l-subdev2.

set -euo pipefail

SCRIPT_PATH=${BASH_SOURCE[0]}
if [[ "$SCRIPT_PATH" != */* ]]; then
  SCRIPT_PATH=$(command -v -- "$SCRIPT_PATH" || true)
fi
if [[ "$SCRIPT_PATH" == */* ]]; then
  SCRIPT_DIR=${SCRIPT_PATH%/*}
else
  SCRIPT_DIR=.
fi
SCRIPT_DIR=$(cd "$SCRIPT_DIR" && pwd)

# Default EDID in this directory. Override with --edid or the VIDEDID environment variable.
EDID_NAME=${VIDEDID:-1080p60edid}
DO_TEST=1
DRY_RUN=0
WIDTH_OVERRIDE=""
HEIGHT_OVERRIDE=""
OUTPUT=csitest.yuv
MEDIA_DIR=${MEDIA_DIR:-/dev}
if [[ "$MEDIA_DIR" != "/" ]]; then
  MEDIA_DIR=${MEDIA_DIR%/}
fi
MEDIA_NODE=""
SLEEP_SECONDS=${SLEEP_SECONDS:-5}

usage() {
  cat <<EOF
Usage: enablehdmi.sh [options] [edid-file]

Load an EDID onto the TC358743 HDMI bridge, then set the CSI-2 pipeline to the
picture the source is sending.

Options:
  --edid NAME     EDID file name in this directory, or a path to another file.
                  Default: ${EDID_NAME}
  --media NODE    Media device to use, such as /dev/media1. Default: search
                  ${MEDIA_DIR} for a node whose graph contains tc358743.
  --width N       Picture width to use when the source reports no timings.
  --height N      Picture height to use when the source reports no timings.
  --output FILE   Frame-test file. Default: csitest.yuv in the current directory.
  --no-test       Configure the pipeline and do not capture test frames.
  --dry-run       Print the setup commands and do not change the device.
  -h, --help      Show this help.

EDID files shipped here: 720p60edid, 1080p25edid, 1080p30edid, 1080p50edid,
1080p60edid. Each file asks the source for that one mode.

Environment:
  VIDEDID         Same as --edid when --edid is not passed.
  MEDIA_DIR       Directory to scan for media nodes. Default: /dev.
  SLEEP_SECONDS   Seconds to wait after loading the EDID. Default: 5.

Exit status:
  0  The source reported timings and the requested steps finished.
  1  Usage error, missing tool, or a setup command failed.
  2  The EDID was loaded and the pipeline was set, but no HDMI timings were detected.
EOF
}

die() {
  echo "enablehdmi: $*" >&2
  exit 1
}

is_whole_number() {
  [[ "$1" =~ ^[0-9]+$ ]]
}

is_picture_size() {
  [[ "$1" =~ ^[1-9][0-9]{0,4}$ ]]
}

nominal_size() {
  local base=${1##*/}
  NOM_W=""
  NOM_H=""
  case "$base" in
    720p60edid) NOM_W=1280; NOM_H=720 ;;
    1080p25edid | 1080p30edid | 1080p50edid | 1080p60edid) NOM_W=1920; NOM_H=1080 ;;
  esac
}

resolve_edid() {
  local name=$1 parent base
  if [[ "$name" == */* ]]; then
    if [[ ! -f "$name" ]]; then
      die "EDID file not found: $name"
    fi
    parent=${name%/*}
    base=${name##*/}
    EDID_PATH=$(cd "$parent" && pwd)/$base
  else
    EDID_PATH=$SCRIPT_DIR/$name
    if [[ ! -f "$EDID_PATH" ]]; then
      die "EDID file not found: $EDID_PATH"
    fi
  fi
  if [[ "$EDID_PATH" == *,* ]]; then
    die "The EDID path cannot contain a comma. v4l2-ctl splits --set-edid on commas."
  fi
  case "$EDID_PATH" in
    *$'\n'* | *$'\r'*)
      die "The EDID path cannot contain a newline."
      ;;
  esac
}

# Keep a selected node inside root. A symlink is allowed only when its target
# stays inside that same directory, so a link cannot point at another file tree.
node_inside() {
  local path=$1 root=$2 target
  case "$path" in
    "$root" | "$root"/*) ;;
    *) die "Refusing ${path}. It is outside ${root}." ;;
  esac
  case "$path" in
    *"/../"* | ../* | */..) die "Refusing ${path}." ;;
  esac
  if [[ -L "$path" ]]; then
    target=$(readlink -f -- "$path") || die "Cannot resolve ${path}."
    case "$target" in
      "$root"/*) ;;
      *) die "Refusing ${path} because it points outside ${root}." ;;
    esac
  fi
}

require_v4l_node() {
  local path=$1
  [[ "$path" =~ ^/dev/(video|v4l-subdev)[0-9]+$ ]] || die "Refusing device path: ${path}"
}

require_entity_name() {
  local name=$1
  [[ "$name" =~ ^[A-Za-z0-9._:-]+([[:space:]][A-Za-z0-9._:-]+)*$ ]] || die "Refusing device name: ${name}"
}

require_output_path() {
  if [[ -L "$OUTPUT" ]]; then
    die "Refusing to write the frame test through a symlink."
  fi
  case "$OUTPUT" in
    /dev/null) ;;
    /dev/*) die "Refusing to write the frame test to a device node." ;;
  esac
}

parse_args() {
  local positional=0
  while [[ $# -gt 0 ]]; do
    case "$1" in
      -h | --help)
        usage
        exit 0
        ;;
      --edid)
        [[ $# -ge 2 ]] || die "--edid needs a file name."
        EDID_NAME=$2
        shift 2
        ;;
      --media)
        [[ $# -ge 2 ]] || die "--media needs a device path."
        MEDIA_NODE=$2
        shift 2
        ;;
      --width)
        [[ $# -ge 2 ]] || die "--width needs a number."
        WIDTH_OVERRIDE=$2
        shift 2
        ;;
      --height)
        [[ $# -ge 2 ]] || die "--height needs a number."
        HEIGHT_OVERRIDE=$2
        shift 2
        ;;
      --output)
        [[ $# -ge 2 ]] || die "--output needs a file path."
        OUTPUT=$2
        shift 2
        ;;
      --no-test)
        DO_TEST=0
        shift
        ;;
      --dry-run)
        DRY_RUN=1
        shift
        ;;
      --)
        shift
        break
        ;;
      -*)
        die "Unknown option: $1"
        ;;
      *)
        if [[ "$positional" -eq 1 ]]; then
          die "Unexpected argument: $1"
        fi
        EDID_NAME=$1
        positional=1
        shift
        ;;
    esac
  done
  if [[ $# -gt 0 ]]; then
    die "Unexpected argument: $1"
  fi
}

print_cmd() {
  local arg
  printf '+'
  for arg in "$@"; do
    if [[ "$arg" == *[[:space:]]* ]]; then
      printf ' "%s"' "$arg"
    else
      printf ' %s' "$arg"
    fi
  done
  printf '\n'
}

run() {
  print_cmd "$@"
  if [[ "$DRY_RUN" -eq 1 ]]; then
    return 0
  fi
  "$@"
}

parse_active_size() {
  local text=$1
  ACTIVE_W=""
  ACTIVE_H=""
  if [[ "$text" =~ Active[[:space:]]width:[[:space:]]*([0-9]+) ]]; then
    ACTIVE_W=${BASH_REMATCH[1]}
  fi
  if [[ "$text" =~ Active[[:space:]]height:[[:space:]]*([0-9]+) ]]; then
    ACTIVE_H=${BASH_REMATCH[1]}
  fi
}

parse_media_graph() {
  local line entity="" pad="" dev target index chosen fallback
  local -a link_from=() link_pad=() link_to=()
  BRIDGE_ENTITY=""
  BRIDGE_DEV=""
  VIDEO_ENTITY=""
  VIDEO_DEV=""
  CSI_PAD=""
  PARSE_ERROR=""
  ENTITIES=()
  unset NODE_OF
  declare -gA NODE_OF=()

  local entity_re='^-[[:space:]]*entity[[:space:]]+[0-9]+:[[:space:]]+(.+)[[:space:]]+\([0-9]+[[:space:]]+pads?,[[:space:]]+[0-9]+[[:space:]]+links?\)$'
  local node_re='device[[:space:]]node[[:space:]]name[[:space:]]([^[:space:]]+)'
  local pad_re='pad([0-9]+):'
  local link_re='->[[:space:]]+"([^"]+)":([0-9]+)'

  while IFS= read -r line || [[ -n "$line" ]]; do
    line=${line%$'\r'}
    if [[ "$line" =~ $entity_re ]]; then
      entity=${BASH_REMATCH[1]}
      pad=""
      ENTITIES+=("$entity")
      if [[ "$entity" == tc358743* ]]; then
        BRIDGE_ENTITY=$entity
      fi
      continue
    fi
    if [[ -n "$entity" && "$line" =~ $node_re ]]; then
      dev=${BASH_REMATCH[1]}
      NODE_OF["$entity"]=$dev
      if [[ "$entity" == "$BRIDGE_ENTITY" ]]; then
        BRIDGE_DEV=$dev
      fi
      continue
    fi
    if [[ "$line" =~ $pad_re ]]; then
      pad=${BASH_REMATCH[1]}
      continue
    fi
    if [[ -n "$entity" && -n "$pad" && "$line" =~ $link_re ]]; then
      link_from+=("$entity")
      link_pad+=("$pad")
      link_to+=("${BASH_REMATCH[1]}")
    fi
  done <<<"$1"

  if [[ -z "$BRIDGE_ENTITY" || -z "$BRIDGE_DEV" ]]; then
    PARSE_ERROR="The media graph has no TC358743 subdevice."
    return 1
  fi

  chosen=-1
  fallback=-1
  if [[ ${#link_from[@]} -gt 0 ]]; then
    for index in "${!link_from[@]}"; do
      [[ "${link_from[$index]}" == "csi2" ]] || continue
      target=${link_to[$index]}
      dev=${NODE_OF[$target]:-}
      [[ "$dev" == /dev/video* ]] || continue
      if [[ "$target" == *ch0* ]]; then
        chosen=$index
        break
      fi
      if [[ "$fallback" -lt 0 ]]; then
        fallback=$index
      fi
    done
  fi
  if [[ "$chosen" -lt 0 ]]; then
    chosen=$fallback
  fi
  if [[ "$chosen" -lt 0 ]]; then
    PARSE_ERROR="The media graph has no video node linked from csi2."
    return 1
  fi
  CSI_PAD=${link_pad[$chosen]}
  VIDEO_ENTITY=${link_to[$chosen]}
  VIDEO_DEV=${NODE_OF[$VIDEO_ENTITY]}
  [[ "$CSI_PAD" =~ ^[0-9]+$ ]] || {
    PARSE_ERROR="The CSI-2 pad is not a number."
    return 1
  }
  return 0
}

find_bridge() {
  local had_nullglob=0 node graph
  local -a nodes=() found=()
  MEDIA_DEV=""
  MEDIA_GRAPH=""
  if shopt -q nullglob; then
    had_nullglob=1
  fi
  shopt -s nullglob
  if [[ -n "$MEDIA_NODE" ]]; then
    nodes=("$MEDIA_NODE")
  else
    nodes=("$MEDIA_DIR"/media[0-9] "$MEDIA_DIR"/media[0-9][0-9])
  fi
  if [[ "$had_nullglob" -eq 0 ]]; then
    shopt -u nullglob
  fi
  if [[ ${#nodes[@]} -eq 0 ]]; then
    die "No media device was found in ${MEDIA_DIR}. Load the tc358743 overlay, then reboot."
  fi
  for node in "${nodes[@]}"; do
    if [[ ! -e "$node" ]]; then
      continue
    fi
    node_inside "$node" "$MEDIA_DIR"
    if graph=$(media-ctl -d "$node" -p 2>/dev/null); then
      if [[ "$graph" == *"tc358743"* ]]; then
        found+=("$node")
        if [[ -z "$MEDIA_DEV" ]]; then
          MEDIA_DEV=$node
          MEDIA_GRAPH=$graph
        fi
      fi
    fi
  done
  if [[ -z "$MEDIA_DEV" ]]; then
    die "No TC358743 bridge was found. If the card is on camera port 0, add cam0 to the overlay. Checked: ${nodes[*]}"
  fi
  if [[ ${#found[@]} -gt 1 ]]; then
    echo "enablehdmi: more than one TC358743 was found (${found[*]}). Using ${MEDIA_DEV}. Pass --media to choose." >&2
  fi
}

list_entities() {
  local name
  if [[ ${#ENTITIES[@]} -eq 0 ]]; then
    return 0
  fi
  echo "Entities on ${MEDIA_DEV}:" >&2
  for name in "${ENTITIES[@]}"; do
    echo "  ${name}" >&2
  done
}

main() {
  local timings="" fmt have_signal=0
  parse_args "$@"
  if ! is_whole_number "$SLEEP_SECONDS"; then
    die "SLEEP_SECONDS must be a whole number of seconds."
  fi
  if [[ -n "$WIDTH_OVERRIDE" ]] && ! is_picture_size "$WIDTH_OVERRIDE"; then
    die "--width must be a positive number."
  fi
  if [[ -n "$HEIGHT_OVERRIDE" ]] && ! is_picture_size "$HEIGHT_OVERRIDE"; then
    die "--height must be a positive number."
  fi
  if [[ -n "$WIDTH_OVERRIDE" || -n "$HEIGHT_OVERRIDE" ]]; then
    if [[ -z "$WIDTH_OVERRIDE" || -z "$HEIGHT_OVERRIDE" ]]; then
      die "Pass both --width and --height."
    fi
  fi
  command -v media-ctl >/dev/null 2>&1 || die "media-ctl is not installed. Install the v4l-utils package."
  command -v v4l2-ctl >/dev/null 2>&1 || die "v4l2-ctl is not installed. Install the v4l-utils package."
  resolve_edid "$EDID_NAME"
  nominal_size "$EDID_PATH"
  require_output_path
  find_bridge
  if ! parse_media_graph "$MEDIA_GRAPH"; then
    list_entities
    die "$PARSE_ERROR"
  fi
  require_v4l_node "$BRIDGE_DEV"
  require_v4l_node "$VIDEO_DEV"
  require_entity_name "$BRIDGE_ENTITY"
  require_entity_name "$VIDEO_ENTITY"

  echo "Media device: ${MEDIA_DEV}"
  echo "Bridge: ${BRIDGE_ENTITY} (${BRIDGE_DEV})"
  echo "Video node: ${VIDEO_DEV}"
  echo "EDID: ${EDID_PATH}"

  run v4l2-ctl -d "$BRIDGE_DEV" --set-edid="file=${EDID_PATH},format=hex" --fix-edid-checksums \
    || die "Could not load the EDID onto ${BRIDGE_DEV}."
  if [[ "$SLEEP_SECONDS" -gt 0 ]]; then
    run sleep "$SLEEP_SECONDS" || die "Could not wait for the HDMI source."
  fi

  print_cmd v4l2-ctl -d "$BRIDGE_DEV" --query-dv-timings
  if timings=$(v4l2-ctl -d "$BRIDGE_DEV" --query-dv-timings 2>&1); then
    parse_active_size "$timings"
    printf '%s\n' "$timings"
    if [[ -n "$ACTIVE_W" && "$ACTIVE_W" -gt 0 && -n "$ACTIVE_H" && "$ACTIVE_H" -gt 0 ]]; then
      have_signal=1
    fi
  else
    printf '%s\n' "$timings" >&2
  fi

  if [[ "$have_signal" -eq 1 ]]; then
    WIDTH=$ACTIVE_W
    HEIGHT=$ACTIVE_H
    if [[ -n "$NOM_W" && ( "$WIDTH" -ne "$NOM_W" || "$HEIGHT" -ne "$NOM_H" ) ]]; then
      echo "The source reported ${WIDTH}x${HEIGHT}. The selected EDID prefers ${NOM_W}x${NOM_H}." >&2
    fi
    run v4l2-ctl -d "$BRIDGE_DEV" --set-dv-bt-timings query \
      || die "Could not apply the detected HDMI timings."
  else
    if [[ -n "$WIDTH_OVERRIDE" ]]; then
      WIDTH=$WIDTH_OVERRIDE
      HEIGHT=$HEIGHT_OVERRIDE
    elif [[ -n "$NOM_W" ]]; then
      WIDTH=$NOM_W
      HEIGHT=$NOM_H
    else
      die "No HDMI timings were detected, and this EDID has no known picture size. Pass --width and --height."
    fi
    echo "No HDMI timings were detected. The pipeline will use ${WIDTH}x${HEIGHT}." >&2
  fi

  fmt="RGB888_1X24/${WIDTH}x${HEIGHT} field:none colorspace:srgb"
  echo "Picture: ${WIDTH}x${HEIGHT} RGB"
  run media-ctl -d "$MEDIA_DEV" -r \
    || die "Could not reset links on ${MEDIA_DEV}."
  run media-ctl -d "$MEDIA_DEV" -l "'csi2':${CSI_PAD} -> '${VIDEO_ENTITY}':0 [1]" \
    || die "Could not enable the CSI-2 link."
  run media-ctl -d "$MEDIA_DEV" -V "'${BRIDGE_ENTITY}':0 [fmt:${fmt}]" \
    || die "Could not set the bridge format."
  run media-ctl -d "$MEDIA_DEV" -V "'csi2':0 [fmt:${fmt}]" \
    || die "Could not set the CSI-2 sink format."
  run media-ctl -d "$MEDIA_DEV" -V "'csi2':${CSI_PAD} [fmt:${fmt}]" \
    || die "Could not set the CSI-2 source format."
  run v4l2-ctl -d "$VIDEO_DEV" --set-fmt-video="width=${WIDTH},height=${HEIGHT},pixelformat=RGB3" \
    || die "Could not set the video format on ${VIDEO_DEV}."

  if [[ "$have_signal" -eq 0 ]]; then
    echo "The EDID is loaded. Connect the HDMI source and run this script again so the detected size can be applied." >&2
    exit 2
  fi
  if [[ "$DO_TEST" -eq 0 || "$DRY_RUN" -eq 1 ]]; then
    return 0
  fi

  run v4l2-ctl --verbose -d "$VIDEO_DEV" \
    --set-fmt-video="width=${WIDTH},height=${HEIGHT},pixelformat=RGB3" \
    --stream-mmap=4 --stream-skip=3 --stream-count=2 \
    --stream-to="$OUTPUT" --stream-poll \
    || die "The frame test failed."
  if [[ "$OUTPUT" != /dev/null && ! -s "$OUTPUT" ]]; then
    die "The frame test wrote no data to ${OUTPUT}."
  fi
  if [[ "$OUTPUT" != /dev/null ]]; then
    echo "Wrote $(wc -c <"$OUTPUT" | tr -d '[:space:]') bytes to ${OUTPUT}"
  fi
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
