# HDMI input on a Raspberry Pi 5

This repository configures a TC358743 HDMI to CSI-2 card on a Raspberry Pi 5.
The card shows up as a camera. The new setup path is the `hdmi-in` command. It finds the media device after each boot, loads an EDID, and sets the capture format from the picture the source is sending.

`hdmi2csi2card/enablehdmi.sh` is the earlier shell path. It does the same job by calling `v4l2-ctl` and `media-ctl`. Keep it. The Rust command talks to the kernel with ioctls and does not call those tools.

The command, the script, and the EDID files have tests. A live card was not attached while those tests ran. Anything that needs the Pi is marked untested below.

## What you need

- Raspberry Pi 5
- TC358743 HDMI to CSI-2 card on a camera port
- 4 lane ribbon if you want 1080p50 or 1080p60 RGB
- `v4l-utils` only if you still run the shell script

```
sudo usermod -aG video "$USER"
```

Log in again after the group change. The account that runs `hdmi-in` needs access to video devices.

## Boot configuration

On current images edit `/boot/firmware/config.txt`. On an older image the same file is `/boot/config.txt`.

```
dtparam=i2c_arm=on
dtparam=i2s=on
dtparam=i2c_arm_baudrate=10000
dtparam=i2c_vc=on

camera_auto_detect=0

dtoverlay=vc4-kms-v3d,cma-512
max_framebuffers=2

[all]
dtoverlay=tc358743,4lane=1
dtoverlay=tc358743-audio
```

- `camera_auto_detect=0` is required. Automatic camera detection fights this overlay.
- `i2c_arm_baudrate=10000` slows the camera bus. This bridge often fails at the default speed. Older images accept the same setting as `dtparam=i2c_baudrate=10000`.
- `i2s=on` and `tc358743-audio` are for HDMI audio. Leave both out if you do not need audio.
- `4lane=1` uses four CSI-2 data lanes. 1080p50 and 1080p60 RGB need four lanes. 720p60, 1080p25, and 1080p30 can work on two lanes if you remove `4lane=1` and the source matches that lower rate.
- The overlay uses camera port 1 unless you add `cam0`. If the card is on camera port 0, use `dtoverlay=tc358743,4lane=1,cam0`.
- `cma-512` reserves 512 MB for display and camera buffers.
- SPI is not used by this card.

Reboot after saving the file.

## Install the command

The release binary is built for aarch64, the 64-bit system image on the Pi 5. These install steps were not run on a Pi here. Treat them as untested until you try them.

Download the `hdmi-in-aarch64` file attached to the test workflow, copy it to the Pi, then:

```
chmod +x hdmi-in
```

Or build it on the Pi, if `cargo` is installed:

```
cargo build --release
```

That writes `target/release/hdmi-in`.

Or build it on another machine and copy the file across:

```
sudo apt-get install -y gcc-aarch64-linux-gnu
rustup target add aarch64-unknown-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu
```

That writes `target/aarch64-unknown-linux-gnu/release/hdmi-in`.

## Run it on the Pi

These commands are untested on hardware. Connect the HDMI source first. When you report results, paste the full output of each command, including the exit line.

Read the card without changing it:

```
./hdmi-in status
echo "status exit: $?"
```

Preview the 1080p60 setup without programming the device:

```
./hdmi-in --dry-run --edid 1080p60edid
echo "dry-run exit: $?"
```

Ask the source for 720p60, 1080p30, and 1080p60. Each command loads that EDID, waits 5 seconds, and, when the source reports timings, writes two RGB frames to `csitest.yuv`.

```
./hdmi-in --edid 720p60edid
echo "720p60 exit: $?"
./hdmi-in --edid 1080p30edid
echo "1080p30 exit: $?"
./hdmi-in --edid 1080p60edid
echo "1080p60 exit: $?"
./hdmi-in status
echo "status exit: $?"
```

Exit status 0 means the source reported timings and the requested steps finished. Exit status 2 means the EDID was loaded and a nominal picture size was programmed, but the source had not reported timings. Exit status 1 means the command stopped on an error. `status` exits 0 when it prints the report, including when `signal_lock` is `no`. It exits 1 when no bridge is found.

`status` prints the media device, the bridge, the video node, the link pads, the bus format, the detected timings, and signal lock. Paste that block back as the hardware report.

The default EDID asks the source for 1920x1080 at 60 Hz. The built-in names match the files in `hdmi2csi2card/`:

| File | Picture the source is asked for |
| --- | --- |
| `720p60edid` | 1280x720 at 60 Hz |
| `1080p25edid` | 1920x1080 at 25 Hz |
| `1080p30edid` | 1920x1080 at 30 Hz |
| `1080p50edid` | 1920x1080 at 50 Hz |
| `1080p60edid` | 1920x1080 at 60 Hz |

```
./hdmi-in --edid 720p60edid
```

You can pass an EDID name as the trailing argument, or set `VIDEDID` in the environment. A path loads that file. A bare name uses the built-in copy of the matching file. The command checks the header and each block checksum, and it refuses a bad file. It does not rewrite the checksum.

Each file has one preferred timing, an HDMI audio block, and stereo speakers. They do not advertise YCbCr, because the pipeline is set to RGB.

The command prints the media device, the bridge subdevice, and the video node it chose. Those numbers change from boot to boot. Later capture commands should use the video node from that output.

`--no-test` configures the pipeline and does not capture frames. `--dry-run` prints the planned changes and does not change the device. `--media /dev/mediaN` picks one media device when more than one bridge is present. `--help` lists the flags.

```
./hdmi-in --help
```

## Shell script

The shell script is still the same setup, through `v4l2-ctl` and `media-ctl`. Install `v4l-utils` first if you use it.

```
sudo apt-get install -y v4l-utils
bash hdmi2csi2card/enablehdmi.sh --edid 720p60edid
```

It uses the same flags and the same exit statuses. The script can rewrite a bad EDID checksum before loading it. `hdmi-in` refuses a bad checksum.

## When setup fails

- No `/dev/mediaN` node: the overlay did not load. Check `config.txt`, the ribbon, and `dmesg` for `tc358743`.
- Media nodes exist but the bridge is missing: the card is on the other camera port, or the probe failed. The slow I2C speed above is the usual probe fix. Pass `--media /dev/mediaN` if more than one bridge is present.
- Exit status 2: the cable, the source power, or the EDID mode does not match what the source will send. Connect the source and run the command again.
- The source size does not match the EDID name: the command keeps the detected size and prints both. Some sources ignore EDID and keep an older mode.
- The frame test writes no data: try `1080p30edid` or `720p60edid` before `1080p60edid`. Four lane RGB at 1080p60 needs a clean ribbon and a source that actually locks to that mode.

## Read a frame

`csitest.yuv` is packed RGB, 3 bytes per pixel, with no file header. The width and height are the picture size the command printed.

To open the live node, install `python3-opencv` and use the video node from the command output in place of `/dev/videoX`:

```
import cv2
cap = cv2.VideoCapture("/dev/videoX", cv2.CAP_V4L2)
ok, frame = cap.read()
print(ok, None if frame is None else frame.shape)
```

This example was not run on hardware here. If red and blue are swapped, convert the frame before you save or show it.

## Screenshots

Script run:

![Script run](HDMICARDIMG.png)

Reading frames with OpenCV:

![OpenCV preview](HDMICARDIMGOPENCV.png)

## Tests

Install `shellcheck` and `edid-decode`, then run:

```
shellcheck -x hdmi2csi2card/enablehdmi.sh tests/test_enablehdmi.sh
bash tests/test_enablehdmi.sh
python3 tests/test_edid.py
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`python3 hdmi2csi2card/build_edid.py` rewrites the EDID text files. `python3 hdmi2csi2card/build_edid.py --check` fails if those files drift from the generator. The workflow also builds `hdmi-in` for aarch64 and attaches that binary as `hdmi-in-aarch64`.
