//! Layouts that match the Linux media and V4L2 headers on 64-bit targets.
//!
//! Sizes were checked against the kernel headers with both the host compiler
//! and `aarch64-linux-gnu-gcc`. The two targets agreed.

use std::mem::MaybeUninit;

pub const MEDIA_ENT_ID_FLAG_NEXT: u32 = 1 << 31;
pub const MEDIA_LNK_FL_ENABLED: u32 = 1;
pub const MEDIA_LNK_FL_IMMUTABLE: u32 = 1 << 1;
pub const MEDIA_LNK_FL_LINK_TYPE: u32 = 0xf << 28;

pub const V4L2_FIELD_NONE: u32 = 1;
pub const V4L2_BUF_TYPE_VIDEO_CAPTURE: u32 = 1;
pub const V4L2_MEMORY_MMAP: u32 = 1;
pub const V4L2_COLORSPACE_SRGB: u32 = 8;
pub const V4L2_SUBDEV_FORMAT_ACTIVE: u32 = 1;
pub const MEDIA_BUS_FMT_RGB888_1X24: u32 = 0x100a;
pub const V4L2_PIX_FMT_RGB24: u32 = u32::from_le_bytes(*b"RGB3");
pub const V4L2_CAP_VIDEO_CAPTURE: u32 = 0x0000_0001;
pub const V4L2_CAP_VIDEO_CAPTURE_MPLANE: u32 = 0x0000_1000;
pub const V4L2_CAP_DEVICE_CAPS: u32 = 0x8000_0000;

const DIR_WRITE: u32 = 1;
const DIR_READ: u32 = 2;
const DIR_RW: u32 = DIR_READ | DIR_WRITE;

const fn ioc(dir: u32, ty: u8, nr: u8, size: u32) -> u32 {
    (dir << 30) | (size << 16) | ((ty as u32) << 8) | (nr as u32)
}

pub const VIDIOC_QUERYCAP: u32 = ioc(DIR_READ, b'V', 0, 104);
pub const VIDIOC_SUBDEV_G_FMT: u32 = ioc(DIR_RW, b'V', 4, 88);
pub const VIDIOC_SUBDEV_S_FMT: u32 = ioc(DIR_RW, b'V', 5, 88);
pub const VIDIOC_G_FMT: u32 = ioc(DIR_RW, b'V', 4, 208);
pub const VIDIOC_S_FMT: u32 = ioc(DIR_RW, b'V', 5, 208);
pub const VIDIOC_REQBUFS: u32 = ioc(DIR_RW, b'V', 8, 20);
pub const VIDIOC_QUERYBUF: u32 = ioc(DIR_RW, b'V', 9, 88);
pub const VIDIOC_QBUF: u32 = ioc(DIR_RW, b'V', 15, 88);
pub const VIDIOC_DQBUF: u32 = ioc(DIR_RW, b'V', 17, 88);
pub const VIDIOC_STREAMON: u32 = ioc(DIR_WRITE, b'V', 18, 4);
pub const VIDIOC_STREAMOFF: u32 = ioc(DIR_WRITE, b'V', 19, 4);
pub const VIDIOC_S_EDID: u32 = ioc(DIR_RW, b'V', 41, 40);
pub const VIDIOC_S_DV_TIMINGS: u32 = ioc(DIR_RW, b'V', 87, 132);
pub const VIDIOC_QUERY_DV_TIMINGS: u32 = ioc(DIR_READ, b'V', 99, 132);
pub const MEDIA_IOC_ENUM_ENTITIES: u32 = ioc(DIR_RW, b'|', 0x01, 256);
pub const MEDIA_IOC_ENUM_LINKS: u32 = ioc(DIR_RW, b'|', 0x02, 40);
pub const MEDIA_IOC_SETUP_LINK: u32 = ioc(DIR_RW, b'|', 0x03, 52);

pub fn zeroed<T>() -> T {
    unsafe { MaybeUninit::<T>::zeroed().assume_init() }
}

#[repr(C)]
pub struct V4l2Edid {
    pub pad: u32,
    pub start_block: u32,
    pub blocks: u32,
    pub reserved: [u32; 5],
    pub edid: *mut u8,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct BtTimings {
    pub width: u32,
    pub height: u32,
    pub interlaced: u32,
    pub polarities: u32,
    pub pixelclock: u64,
    pub hfrontporch: u32,
    pub hsync: u32,
    pub hbackporch: u32,
    pub vfrontporch: u32,
    pub vsync: u32,
    pub vbackporch: u32,
    pub il_vfrontporch: u32,
    pub il_vsync: u32,
    pub il_vbackporch: u32,
    pub standards: u32,
    pub flags: u32,
    pub picture_aspect_numerator: u32,
    pub picture_aspect_denominator: u32,
    pub cea861_vic: u8,
    pub hdmi_vic: u8,
    pub reserved: [u8; 46],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct DvTimings {
    pub type_: u32,
    pub data: [u8; 128],
}

impl DvTimings {
    pub fn bt(&self) -> BtTimings {
        let mut bytes = [0u8; 124];
        bytes.copy_from_slice(&self.data[..124]);
        unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const BtTimings) }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SubdevFormat {
    pub which: u32,
    pub pad: u32,
    pub width: u32,
    pub height: u32,
    pub code: u32,
    pub field: u32,
    pub colorspace: u32,
    pub ycbcr_enc: u16,
    pub quantization: u16,
    pub xfer_func: u16,
    pub mbus_flags: u16,
    pub mbus_reserved: [u16; 10],
    pub stream: u32,
    pub reserved: [u32; 7],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PixFormat {
    pub width: u32,
    pub height: u32,
    pub pixelformat: u32,
    pub field: u32,
    pub bytesperline: u32,
    pub sizeimage: u32,
    pub colorspace: u32,
    pub priv_: u32,
    pub flags: u32,
    pub ycbcr_enc: u32,
    pub quantization: u32,
    pub xfer_func: u32,
}

#[repr(C, align(8))]
pub struct V4l2Format {
    pub type_: u32,
    pub raw: [u8; 200],
    pub _tail: u32,
}

fn put_u32(raw: &mut [u8], offset: usize, value: u32) {
    raw[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
}

fn get_u32(raw: &[u8], offset: usize) -> u32 {
    u32::from_ne_bytes(raw[offset..offset + 4].try_into().expect("4 bytes"))
}

impl V4l2Format {
    pub fn pix(&self) -> PixFormat {
        PixFormat {
            width: get_u32(&self.raw, 0),
            height: get_u32(&self.raw, 4),
            pixelformat: get_u32(&self.raw, 8),
            field: get_u32(&self.raw, 12),
            bytesperline: get_u32(&self.raw, 16),
            sizeimage: get_u32(&self.raw, 20),
            colorspace: get_u32(&self.raw, 24),
            priv_: get_u32(&self.raw, 28),
            flags: get_u32(&self.raw, 32),
            ycbcr_enc: get_u32(&self.raw, 36),
            quantization: get_u32(&self.raw, 40),
            xfer_func: get_u32(&self.raw, 44),
        }
    }

    pub fn set_pix(&mut self, pix: &PixFormat) {
        put_u32(&mut self.raw, 0, pix.width);
        put_u32(&mut self.raw, 4, pix.height);
        put_u32(&mut self.raw, 8, pix.pixelformat);
        put_u32(&mut self.raw, 12, pix.field);
        put_u32(&mut self.raw, 16, pix.bytesperline);
        put_u32(&mut self.raw, 20, pix.sizeimage);
        put_u32(&mut self.raw, 24, pix.colorspace);
        put_u32(&mut self.raw, 28, pix.priv_);
        put_u32(&mut self.raw, 32, pix.flags);
        put_u32(&mut self.raw, 36, pix.ycbcr_enc);
        put_u32(&mut self.raw, 40, pix.quantization);
        put_u32(&mut self.raw, 44, pix.xfer_func);
    }
}

#[repr(C)]
pub struct RequestBuffers {
    pub count: u32,
    pub type_: u32,
    pub memory: u32,
    pub capabilities: u32,
    pub flags: u8,
    pub reserved: [u8; 3],
}

/// Single-plane `struct v4l2_buffer` with the padding both 64-bit targets insert.
#[repr(C)]
pub struct V4l2Buffer {
    pub index: u32,
    pub type_: u32,
    pub bytesused: u32,
    pub flags: u32,
    pub field: u32,
    pub _pad_before_timestamp: u32,
    pub timestamp: [u8; 16],
    pub timecode_type: u32,
    pub timecode_flags: u32,
    pub frames: u8,
    pub seconds: u8,
    pub minutes: u8,
    pub hours: u8,
    pub userbits: [u8; 4],
    pub sequence: u32,
    pub memory: u32,
    pub offset: u32,
    pub _pad_mmap_union: u32,
    pub length: u32,
    pub reserved2: u32,
    pub request_fd: i32,
    pub _tail: u32,
}

#[repr(C)]
pub struct Capability {
    pub driver: [u8; 16],
    pub card: [u8; 32],
    pub bus_info: [u8; 32],
    pub version: u32,
    pub capabilities: u32,
    pub device_caps: u32,
    pub reserved: [u32; 3],
}

#[repr(C)]
pub struct MediaEntityDesc {
    pub id: u32,
    pub name: [u8; 32],
    pub type_: u32,
    pub revision: u32,
    pub flags: u32,
    pub group_id: u32,
    pub pads: u16,
    pub links: u16,
    pub reserved: [u32; 4],
    pub major: u32,
    pub minor: u32,
    pub raw_rest: [u8; 176],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MediaPadDesc {
    pub entity: u32,
    pub index: u16,
    pub _pad: u16,
    pub flags: u32,
    pub reserved: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MediaLinkDesc {
    pub source: MediaPadDesc,
    pub sink: MediaPadDesc,
    pub flags: u32,
    pub reserved: [u32; 2],
}

#[repr(C)]
pub struct MediaLinksEnum {
    pub entity: u32,
    pub _pad: u32,
    pub pads: *mut MediaPadDesc,
    pub links: *mut MediaLinkDesc,
    pub reserved: [u32; 4],
}

pub fn c_string(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

pub fn ioctl_raw<T>(fd: i32, request: u32, arg: *mut T) -> std::io::Result<()> {
    loop {
        let rc = unsafe { libc::ioctl(fd, request as libc::Ioctl, arg) };
        if rc >= 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        return Err(error);
    }
}

pub fn ioctl<T>(fd: i32, request: u32, arg: *mut T, what: &str) -> crate::error::Result<()> {
    ioctl_raw(fd, request, arg).map_err(|error| crate::error::err(format!("{what}: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn struct_sizes_match_kernel_headers() {
        assert_eq!(size_of::<V4l2Edid>(), 40);
        assert_eq!(size_of::<BtTimings>(), 124);
        assert_eq!(size_of::<DvTimings>(), 132);
        assert_eq!(size_of::<SubdevFormat>(), 88);
        assert_eq!(size_of::<PixFormat>(), 48);
        assert_eq!(size_of::<V4l2Format>(), 208);
        assert_eq!(size_of::<RequestBuffers>(), 20);
        assert_eq!(size_of::<V4l2Buffer>(), 88);
        assert_eq!(size_of::<Capability>(), 104);
        assert_eq!(size_of::<MediaEntityDesc>(), 256);
        assert_eq!(size_of::<MediaPadDesc>(), 20);
        assert_eq!(size_of::<MediaLinkDesc>(), 52);
        assert_eq!(size_of::<MediaLinksEnum>(), 40);
    }

    #[test]
    fn buffer_field_offsets_match_kernel_headers() {
        assert_eq!(std::mem::offset_of!(V4l2Buffer, timestamp), 24);
        assert_eq!(std::mem::offset_of!(V4l2Buffer, sequence), 56);
        assert_eq!(std::mem::offset_of!(V4l2Buffer, memory), 60);
        assert_eq!(std::mem::offset_of!(V4l2Buffer, offset), 64);
        assert_eq!(std::mem::offset_of!(V4l2Buffer, length), 72);
    }

    #[test]
    fn ioctl_numbers_match_kernel_headers() {
        assert_eq!(VIDIOC_S_EDID, 0xc028_5629);
        assert_eq!(VIDIOC_SUBDEV_S_FMT, 0xc058_5605);
        assert_eq!(VIDIOC_SUBDEV_G_FMT, 0xc058_5604);
        assert_eq!(VIDIOC_S_DV_TIMINGS, 0xc084_5657);
        assert_eq!(VIDIOC_QUERY_DV_TIMINGS, 0x8084_5663);
        assert_eq!(VIDIOC_S_FMT, 0xc0d0_5605);
        assert_eq!(VIDIOC_G_FMT, 0xc0d0_5604);
        assert_eq!(VIDIOC_REQBUFS, 0xc014_5608);
        assert_eq!(VIDIOC_QUERYBUF, 0xc058_5609);
        assert_eq!(VIDIOC_QBUF, 0xc058_560f);
        assert_eq!(VIDIOC_DQBUF, 0xc058_5611);
        assert_eq!(VIDIOC_STREAMON, 0x4004_5612);
        assert_eq!(VIDIOC_STREAMOFF, 0x4004_5613);
        assert_eq!(MEDIA_IOC_ENUM_ENTITIES, 0xc100_7c01);
        assert_eq!(MEDIA_IOC_ENUM_LINKS, 0xc028_7c02);
        assert_eq!(MEDIA_IOC_SETUP_LINK, 0xc034_7c03);
        assert_eq!(V4L2_PIX_FMT_RGB24, 0x3342_4752);
    }
}
