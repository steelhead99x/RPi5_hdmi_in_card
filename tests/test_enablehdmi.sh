#!/usr/bin/env bash
# Exercise enablehdmi.sh with stubbed v4l2-ctl and media-ctl. No capture card is used.

set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
SCRIPT=$ROOT/hdmi2csi2card/enablehdmi.sh
FIXTURE=$ROOT/tests/fixtures/media-graph.txt

# shellcheck source=hdmi2csi2card/enablehdmi.sh
source "$SCRIPT"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_eq() {
  if [[ "$1" != "$2" ]]; then
    echo "FAIL: $3" >&2
    echo "  got: $1" >&2
    echo "  expected: $2" >&2
    exit 1
  fi
}

parse_media_graph "$(cat "$FIXTURE")" || fail "parser rejected the sample graph"
assert_eq "$BRIDGE_ENTITY" "tc358743 11-000f" "bridge entity"
assert_eq "$BRIDGE_DEV" "/dev/v4l-subdev7" "bridge node"
assert_eq "$VIDEO_ENTITY" "rp1-cfe-csi2_ch0" "image channel"
assert_eq "$VIDEO_DEV" "/dev/video33" "video node"
assert_eq "$CSI_PAD" "4" "csi2 source pad"

fallback_graph=$(cat <<'EOF'
- entity 1: csi2 (8 pads, 2 links)
            device node name /dev/v4l-subdev0
	pad6: Source
		-> "rp1-cfe-csi2_ch2":0 []
- entity 2: tc358743 4-000f (1 pad, 1 link)
            device node name /dev/v4l-subdev4
	pad0: Source
		-> "csi2":0 [ENABLED,IMMUTABLE]
- entity 3: rp1-cfe-csi2_ch2 (1 pad, 1 link)
            device node name /dev/video8
	pad0: Sink
EOF
)
parse_media_graph "$fallback_graph" || fail "parser rejected a graph without ch0"
assert_eq "$VIDEO_DEV" "/dev/video8" "fallback video node"
assert_eq "$CSI_PAD" "6" "fallback pad"
assert_eq "$BRIDGE_DEV" "/dev/v4l-subdev4" "fallback bridge node"

if parse_media_graph "- entity 1: csi2 (8 pads, 0 links)"; then
  fail "parser accepted a graph with no bridge"
fi

unset VIDEDID || true
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
STUB=$WORK/bin
mkdir -p "$STUB" "$WORK/dev" "$WORK/empty"
LOG=$WORK/log

cat >"$STUB/media-ctl" <<'EOF'
#!/bin/bash
printf '%s\n' "$*" >>"${LOG:?}"
if [[ "$1" == "-d" && "$3" == "-p" ]]; then
  base=$(basename -- "$2")
  if [[ "$base" == "${BRIDGE_NODE:-media1}" ]]; then
    cat "${FIXTURE:?}"
    exit 0
  fi
  printf '%s\n' "driver rp1-cfe" "- entity 1: csi2 (8 pads, 0 links)"
  exit 0
fi
exit 0
EOF

cat >"$STUB/v4l2-ctl" <<'EOF'
#!/bin/bash
printf '%s\n' "$*" >>"${LOG:?}"
if [[ "$*" == *"--query-dv-timings"* ]]; then
  case "${QUERY_MODE:-ok}" in
    fail)
      echo "VIDIOC_QUERY_DV_TIMINGS: failed: Link has been severed" >&2
      exit 1
      ;;
    zero)
      printf '\tActive width: 0\n\tActive height: 0\n'
      exit 0
      ;;
  esac
  printf '\tActive width: %s\n\tActive height: %s\n' "${SIGNAL_W:-1920}" "${SIGNAL_H:-1080}"
  exit 0
fi
for arg in "$@"; do
  case "$arg" in
    --stream-to=*)
      printf 'frame' >"${arg#--stream-to=}"
      ;;
  esac
done
exit 0
EOF
chmod +x "$STUB/media-ctl" "$STUB/v4l2-ctl"
touch "$WORK/dev/media0" "$WORK/dev/media1"

run_script() {
  : >"$LOG"
  set +e
  timeout 15 env \
    PATH="$STUB:/bin:/usr/bin" \
    MEDIA_DIR="${MEDIA_DIR_TEST:-$WORK/dev}" \
    SLEEP_SECONDS=0 \
    LOG="$LOG" \
    FIXTURE="$FIXTURE" \
    BRIDGE_NODE="${BRIDGE_NODE:-media1}" \
    QUERY_MODE="${QUERY_MODE:-ok}" \
    SIGNAL_W="${SIGNAL_W:-1920}" \
    SIGNAL_H="${SIGNAL_H:-1080}" \
    bash "$SCRIPT" "$@" >"$WORK/out" 2>"$WORK/err"
  status=$?
  set -e
  cat "$WORK/out"
  cat "$WORK/err" >&2
  return "$status"
}

echo "test: detected 720p uses the bridge nodes and hex EDID"
SIGNAL_W=1280 SIGNAL_H=720
run_script --edid 720p60edid --output "$WORK/csitest.yuv"
assert_eq "$status" "0" "720p exit"
grep -F -- "/dev/v4l-subdev7" "$LOG" >/dev/null || fail "did not use the bridge subdevice"
grep -F -- "/dev/video33" "$LOG" >/dev/null || fail "did not use the discovered video node"
grep -F -- "format=hex" "$LOG" >/dev/null || fail "EDID was not loaded as hex"
grep -F -- "$ROOT/hdmi2csi2card/720p60edid" "$LOG" >/dev/null || fail "EDID path is not the script directory"
grep -F -- "RGB888_1X24/1280x720" "$LOG" >/dev/null || fail "pipeline size did not follow the signal"
grep -F -- "tc358743 11-000f" "$LOG" >/dev/null || fail "bridge entity was not configured"
if grep -F -- "/dev/video0" "$LOG" || grep -F -- "/dev/v4l-subdev2" "$LOG"; then
  fail "script still uses a hardcoded video or subdevice node"
fi
[[ -s "$WORK/csitest.yuv" ]] || fail "frame test file is empty"

echo "test: relative invocation still finds the EDID"
: >"$LOG"
set +e
(
  cd /tmp
  rel=$(python3 -c 'import os,sys; print(os.path.relpath(sys.argv[1], sys.argv[2]))' "$SCRIPT" /tmp)
  env \
    PATH="$STUB:/bin:/usr/bin" \
    MEDIA_DIR="$WORK/dev" \
    SLEEP_SECONDS=0 \
    LOG="$LOG" \
    FIXTURE="$FIXTURE" \
    BRIDGE_NODE=media1 \
    QUERY_MODE=ok \
    SIGNAL_W=1280 \
    SIGNAL_H=720 \
    bash "$rel" --edid 720p60edid --output "$WORK/from-tmp.yuv" >"$WORK/out" 2>"$WORK/err"
  echo $? >"$WORK/status"
)
set -e
assert_eq "$(cat "$WORK/status")" "0" "relative invocation exit"
grep -F -- "$ROOT/hdmi2csi2card/720p60edid" "$LOG" >/dev/null || fail "relative run lost the EDID directory"

echo "test: no signal configures the named mode and does not pretend frames arrived"
QUERY_MODE=fail
SIGNAL_W=1920 SIGNAL_H=1080
if run_script --edid 1080p60edid --output "$WORK/nosignal.yuv"; then
  fail "missing signal should not exit 0"
fi
assert_eq "$status" "2" "missing signal exit"
grep -F -- "RGB888_1X24/1920x1080" "$LOG" >/dev/null || fail "nominal 1080p size was not applied"
if grep -F -- "--set-dv-bt-timings" "$LOG"; then
  fail "timings were applied without a signal"
fi
if grep -F -- "--stream-to=" "$LOG"; then
  fail "frame test ran without a signal"
fi
grep -F -- "No HDMI timings" "$WORK/err" >/dev/null || fail "missing-signal message was not printed"

echo "test: a zero-size query is treated as no signal"
QUERY_MODE=zero
if run_script --output "$WORK/zero.yuv"; then
  fail "zero-size timings should not exit 0"
fi
assert_eq "$status" "2" "zero-size exit"

echo "test: mismatched signal is kept and reported"
QUERY_MODE=ok
SIGNAL_W=1920 SIGNAL_H=1080
run_script --edid 720p60edid --no-test --output "$WORK/mismatch.yuv"
assert_eq "$status" "0" "mismatch exit"
grep -F -- "RGB888_1X24/1920x1080" "$LOG" >/dev/null || fail "detected size was replaced with the EDID size"
grep -F -- "1280x720" "$WORK/err" >/dev/null || fail "preferred size was not reported"
if grep -F -- "--stream-to=" "$LOG"; then
  fail "--no-test still captured frames"
fi

echo "test: pinned media node skips the other port"
BRIDGE_NODE=media0
QUERY_MODE=ok
SIGNAL_W=1920 SIGNAL_H=1080
run_script --media "$WORK/dev/media0" --no-test
assert_eq "$status" "0" "pinned media exit"
grep -F -- "$WORK/dev/media0" "$LOG" >/dev/null || fail "pinned media node was not opened"

echo "test: no media node fails quickly"
MEDIA_DIR_TEST=$WORK/empty
if run_script --no-test; then
  fail "empty media directory should fail"
fi
assert_eq "$status" "1" "empty media exit"
grep -F -- "No media device" "$WORK/err" >/dev/null || fail "empty media message missing"
MEDIA_DIR_TEST=$WORK/dev
BRIDGE_NODE=media1

echo "test: media nodes without the bridge fail"
rm -f "$WORK/dev/media1"
if run_script --no-test; then
  fail "a CSI node without the bridge should fail"
fi
assert_eq "$status" "1" "missing bridge exit"
grep -F -- "No TC358743" "$WORK/err" >/dev/null || fail "missing bridge message"
touch "$WORK/dev/media1"

echo "test: unknown EDID and unknown option"
if run_script --edid does-not-exist; then
  fail "missing EDID should fail"
fi
assert_eq "$status" "1" "missing EDID exit"
if run_script --not-an-option; then
  fail "unknown option should fail"
fi
assert_eq "$status" "1" "unknown option exit"

echo "test: help"
help_text=$(bash "$SCRIPT" --help)
grep -F -- "--no-test" <<<"$help_text" >/dev/null || fail "help does not mention --no-test"

echo "test: missing tool"
mkdir -p "$WORK/nopath"
set +e
PATH="$WORK/nopath" SLEEP_SECONDS=0 /usr/bin/bash "$SCRIPT" --no-test >"$WORK/out" 2>"$WORK/err"
status=$?
set -e
assert_eq "$status" "1" "missing tool exit"
grep -E -- "media-ctl|v4l2-ctl" "$WORK/err" >/dev/null || fail "missing tool message"

echo "enablehdmi tests passed"
