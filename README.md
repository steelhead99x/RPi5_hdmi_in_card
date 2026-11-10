# HDMI input on a Raspberry Pi 5

This repository configures a TC358743 HDMI to CSI-2 card on a Raspberry Pi 5.
The card shows up as a camera. `hdmi2csi2card/enablehdmi.sh` finds the media device after each boot, loads an EDID, and sets the capture format from the picture the source is sending.

The script and the EDID files are covered by tests that stub `v4l2-ctl` and `media-ctl`. A live card was not attached while those tests ran, so the picture path still needs a Pi 5 before it can be called working.

## What you need

- Raspberry Pi 5
- TC358743 HDMI to CSI-2 card on a camera port
- 4 lane ribbon if you want 1080p50 or 1080p60 RGB
- `v4l-utils`, which provides `v4l2-ctl` and `media-ctl`

```
sudo apt-get update
sudo apt-get install -y v4l-utils
sudo usermod -aG video "$USER"
```

Log in again after the group change.

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

## Run the script

Copy this repository onto the Pi. Connect the HDMI source, then run:

```
bash hdmi2csi2card/enablehdmi.sh
```

The default EDID asks the source for 1920x1080 at 60 Hz. The other files in `hdmi2csi2card/` are single-mode EDIDs:

| File | Picture the source is asked for |
| --- | --- |
| `720p60edid` | 1280x720 at 60 Hz |
| `1080p25edid` | 1920x1080 at 25 Hz |
| `1080p30edid` | 1920x1080 at 30 Hz |
| `1080p50edid` | 1920x1080 at 50 Hz |
| `1080p60edid` | 1920x1080 at 60 Hz |

```
bash hdmi2csi2card/enablehdmi.sh --edid 720p60edid
```

You can pass an EDID name as the first argument, or set `VIDEDID` in the environment. The script looks up a bare file name next to itself, so the current directory does not matter.

The files are hex text. The script loads them with `format=hex`. Each file has one preferred timing, an HDMI audio block, and stereo speakers. They do not advertise YCbCr, because the pipeline is set to RGB.

The script prints the media device, the bridge subdevice, and the video node it chose. Those numbers change from boot to boot. Later capture commands should use the video node from that output.

By default the script then writes two RGB frames to `csitest.yuv` in the current directory. Skip that with `--no-test`. `--dry-run` prints the setup commands and does not change the device.

```
bash hdmi2csi2card/enablehdmi.sh --help
```

Exit status 0 means the source reported timings and the requested steps finished. Exit status 2 means the EDID was loaded and a nominal picture size was programmed, but the source had not reported timings yet.

## When the script fails

- No `/dev/mediaN` node: the overlay did not load. Check `config.txt`, the ribbon, and `dmesg` for `tc358743`.
- Media nodes exist but the bridge is missing: the card is on the other camera port, or the probe failed. The slow I2C speed above is the usual probe fix. Pass `--media /dev/mediaN` if more than one bridge is present.
- Exit status 2: the cable, the source power, or the EDID mode does not match what the source will send. Connect the source and run the script again.
- The source size does not match the EDID name: the script keeps the detected size and prints both. Some sources ignore EDID and keep an older mode.
- The frame test writes no data: try `1080p30edid` or `720p60edid` before `1080p60edid`. Four lane RGB at 1080p60 needs a clean ribbon and a source that actually locks to that mode.

## Read a frame

`csitest.yuv` is packed RGB, 3 bytes per pixel, with no file header. The width and height are the picture size the script printed.

To open the live node, install `python3-opencv` and use the video node from the script output in place of `/dev/videoX`:

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
```

`python3 hdmi2csi2card/build_edid.py` rewrites the EDID text files. `python3 hdmi2csi2card/build_edid.py --check` fails if those files drift from the generator.
