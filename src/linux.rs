//! Layouts that match the Linux media and V4L2 headers on 64-bit targets.
//!
//! Sizes and field offsets were checked against the kernel headers with the
//! host compiler and `aarch64-linux-gnu-gcc`. The two targets agreed.

use std::mem::MaybeUninit;

pub const MEDIA_ENT_ID_FLAG_NEXT: u32 = 1 << 31;
pub const MEDIA_LNK_FL_ENABLED: u32 = 1;
pub const MEDIA_LNK_FL_IMMUTABLE: u32 = 1 << 1;
pub const MEDIA_LNK_FL_LINK_TYPE: u32 = 0xf << 28;

pub const V4L2_FIELD_NONE: u32 = 1;
pub const V4L2_BUF_TYPE_VIDEO_CAPTURE: u32 = 1;
pub const V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE: u32 = 9;
pub const V4L2_MEMORY_MMAP: u32 = 1;
pub const V4L2_COLORSPACE_SRGB: u32 = 8;
pub const V4L2_SUBDEV_FORMAT_ACTIVE: u32 = 1;
pub const MEDIA_BUS_FMT_RGB888_1X24: u32 = 0x100a;
pub const V4L2_PIX_FMT_RGB24: u32 = u32::from_le_bytes(*b"RGB3");
pub const V4L2_PIX_FMT_BGR24: u32 = u32::from_le_bytes(*b"BGR3");
pub const V4L2_CAP_VIDEO_CAPTURE: u32 = 0x0000_0001;
pub const V4L2_CAP_VIDEO_CAPTURE_MPLANE: u32 = 0x0000_1000;
pub const V4L2_CAP_META_CAPTURE: u32 = 0x0080_0000;
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

/// `struct v4l2_format`. The format union is 8-byte aligned, so four padding
/// bytes sit between `type` and the 200-byte payload.
#[repr(C, align(8))]
pub struct V4l2Format {
    pub type_: u32,
    pub _pad: u32,
    pub raw: [u8; 200],
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
        self.raw = [0; 200];
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

    pub fn mplane(&self) -> MplaneFormat {
        let mut encoded: PixMpRaw = zeroed();
        // SAFETY: `PixMpRaw` is the packed 192-byte kernel layout, copied out of
        // the 200-byte format union. Integer fields are read by value afterwards.
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.raw.as_ptr(),
                &mut encoded as *mut PixMpRaw as *mut u8,
                std::mem::size_of::<PixMpRaw>(),
            );
        }
        MplaneFormat {
            width: encoded.width,
            height: encoded.height,
            pixelformat: encoded.pixelformat,
            field: encoded.field,
            colorspace: encoded.colorspace,
            bytesperline: encoded.plane0_bytesperline,
            sizeimage: encoded.plane0_sizeimage,
            num_planes: encoded.num_planes,
            flags: encoded.flags,
            ycbcr_enc: encoded.ycbcr_enc,
            quantization: encoded.quantization,
            xfer_func: encoded.xfer_func,
        }
    }

    pub fn set_mplane(&mut self, pix: &MplaneFormat) {
        self.raw = [0; 200];
        let encoded = PixMpRaw {
            width: pix.width,
            height: pix.height,
            pixelformat: pix.pixelformat,
            field: pix.field,
            colorspace: pix.colorspace,
            plane0_sizeimage: pix.sizeimage,
            plane0_bytesperline: pix.bytesperline,
            plane0_reserved: [0; 6],
            other_planes: [0; 140],
            num_planes: pix.num_planes,
            flags: pix.flags,
            ycbcr_enc: pix.ycbcr_enc,
            quantization: pix.quantization,
            xfer_func: pix.xfer_func,
            reserved: [0; 7],
        };
        // SAFETY: `PixMpRaw` is packed to 192 bytes and `raw` is 200 bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(
                &encoded as *const PixMpRaw as *const u8,
                self.raw.as_mut_ptr(),
                std::mem::size_of::<PixMpRaw>(),
            );
        }
    }
}

/// One-plane view of `struct v4l2_pix_format_mplane`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MplaneFormat {
    pub width: u32,
    pub height: u32,
    pub pixelformat: u32,
    pub field: u32,
    pub colorspace: u32,
    pub bytesperline: u32,
    pub sizeimage: u32,
    pub num_planes: u8,
    pub flags: u8,
    pub ycbcr_enc: u8,
    pub quantization: u8,
    pub xfer_func: u8,
}

/// Packed `struct v4l2_pix_format_mplane`. Plane 0 is explicit so the other
/// seven planes stay zero without taking a reference to a packed field.
#[repr(C, packed)]
struct PixMpRaw {
    width: u32,
    height: u32,
    pixelformat: u32,
    field: u32,
    colorspace: u32,
    plane0_sizeimage: u32,
    plane0_bytesperline: u32,
    plane0_reserved: [u16; 6],
    other_planes: [u8; 140],
    num_planes: u8,
    flags: u8,
    ycbcr_enc: u8,
    quantization: u8,
    xfer_func: u8,
    reserved: [u8; 7],
}

const _: () = assert!(std::mem::size_of::<PixMpRaw>() == 192);
const _: () = assert!(std::mem::size_of::<PixMpRaw>() <= 200);
const _: () = assert!(std::mem::size_of::<V4l2Format>() == 208);
const _: () = assert!(std::mem::offset_of!(V4l2Format, raw) == 8);
const _: () = assert!(std::mem::size_of::<V4l2Buffer>() == 88);
const _: () = assert!(std::mem::align_of::<V4l2Buffer>() == 8);
const _: () = assert!(std::mem::offset_of!(V4l2Buffer, m) == 64);
const _: () = assert!(std::mem::offset_of!(V4l2Buffer, length) == 72);
const _: () = assert!(std::mem::size_of::<V4l2Plane>() == 64);
const _: () = assert!(std::mem::align_of::<V4l2Plane>() == 8);
const _: () = assert!(std::mem::offset_of!(V4l2Plane, data_offset) == 16);

#[repr(C)]
pub struct RequestBuffers {
    pub count: u32,
    pub type_: u32,
    pub memory: u32,
    pub capabilities: u32,
    pub flags: u8,
    pub reserved: [u8; 3],
}

/// `struct v4l2_buffer`. `m` holds a mmap offset, or a pointer to `v4l2_plane`
/// when the buffer type is multiplanar.
#[repr(C, align(8))]
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
    pub m: BufferMemory,
    pub length: u32,
    pub reserved2: u32,
    pub request_fd: i32,
    pub _tail: u32,
}

#[repr(C)]
pub union BufferMemory {
    pub offset: u32,
    #[allow(dead_code)]
    pub userptr: u64,
    pub planes: *mut V4l2Plane,
    #[allow(dead_code)]
    pub fd: i32,
}

#[repr(C)]
pub union PlaneMemory {
    pub mem_offset: u32,
    #[allow(dead_code)]
    pub userptr: u64,
    #[allow(dead_code)]
    pub fd: i32,
}

/// `struct v4l2_plane`.
#[repr(C)]
pub struct V4l2Plane {
    pub bytesused: u32,
    pub length: u32,
    pub m: PlaneMemory,
    pub data_offset: u32,
    pub reserved: [u32; 11],
}

impl V4l2Buffer {
    pub fn set_planes(&mut self, planes: *mut V4l2Plane) {
        self.m.planes = planes;
    }

    pub fn mmap_offset(&self) -> u32 {
        // SAFETY: a single-planar queue stores a u32 mmap cookie in this word.
        unsafe { self.m.offset }
    }
}

impl V4l2Plane {
    pub fn mmap_offset(&self) -> u32 {
        // SAFETY: the kernel writes the mmap cookie into the first word of `m`.
        unsafe { self.m.mem_offset }
    }
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

pub fn fourcc_label(value: u32) -> String {
    let bytes = value.to_le_bytes();
    if bytes.iter().all(|byte| byte.is_ascii_graphic()) {
        let text = std::str::from_utf8(&bytes).expect("four ascii bytes");
        format!("{text}:0x{value:08x}")
    } else {
        format!("0x{value:08x}")
    }
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
        assert_eq!(std::mem::align_of::<V4l2Buffer>(), 8);
        assert_eq!(size_of::<V4l2Plane>(), 64);
        assert_eq!(std::mem::align_of::<V4l2Plane>(), 8);
        assert_eq!(size_of::<PixMpRaw>(), 192);
        assert_eq!(size_of::<Capability>(), 104);
        assert_eq!(size_of::<MediaEntityDesc>(), 256);
        assert_eq!(size_of::<MediaPadDesc>(), 20);
        assert_eq!(size_of::<MediaLinkDesc>(), 52);
        assert_eq!(size_of::<MediaLinksEnum>(), 40);
    }

    #[test]
    fn buffer_field_offsets_match_kernel_headers() {
        assert_eq!(std::mem::offset_of!(V4l2Format, raw), 8);
        assert_eq!(std::mem::offset_of!(V4l2Buffer, timestamp), 24);
        assert_eq!(std::mem::offset_of!(V4l2Buffer, sequence), 56);
        assert_eq!(std::mem::offset_of!(V4l2Buffer, memory), 60);
        assert_eq!(std::mem::offset_of!(V4l2Buffer, m), 64);
        assert_eq!(std::mem::offset_of!(V4l2Buffer, length), 72);
        assert_eq!(std::mem::offset_of!(V4l2Plane, m), 8);
        assert_eq!(std::mem::offset_of!(V4l2Plane, data_offset), 16);
        assert_eq!(std::mem::offset_of!(PixMpRaw, colorspace), 16);
        assert_eq!(std::mem::offset_of!(PixMpRaw, plane0_bytesperline), 24);
        assert_eq!(std::mem::offset_of!(PixMpRaw, num_planes), 180);
        assert_eq!(std::mem::offset_of!(PixMpRaw, xfer_func), 184);
    }

    #[test]
    fn single_plane_format_round_trips() {
        let mut format: V4l2Format = zeroed();
        format.set_pix(&PixFormat {
            width: 1280,
            height: 720,
            pixelformat: V4L2_PIX_FMT_RGB24,
            field: V4L2_FIELD_NONE,
            bytesperline: 3840,
            sizeimage: 2_764_800,
            colorspace: V4L2_COLORSPACE_SRGB,
            priv_: 0,
            flags: 0,
            ycbcr_enc: 0,
            quantization: 0,
            xfer_func: 0,
        });
        let got = format.pix();
        assert_eq!(got.width, 1280);
        assert_eq!(got.height, 720);
        assert_eq!(got.pixelformat, V4L2_PIX_FMT_RGB24);
        assert_eq!(got.bytesperline, 3840);
        assert_eq!(got.sizeimage, 2_764_800);
        assert_eq!(got.colorspace, V4L2_COLORSPACE_SRGB);
    }

    #[test]
    fn mplane_format_matches_the_kernel_layout() {
        let mut format: V4l2Format = zeroed();
        format.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE;
        format.set_mplane(&MplaneFormat {
            width: 1920,
            height: 1080,
            pixelformat: V4L2_PIX_FMT_RGB24,
            field: V4L2_FIELD_NONE,
            colorspace: V4L2_COLORSPACE_SRGB,
            bytesperline: 5760,
            sizeimage: 6_220_800,
            num_planes: 1,
            flags: 0,
            ycbcr_enc: 0,
            quantization: 0,
            xfer_func: 0,
        });
        let decoded = format.mplane();
        assert_eq!(decoded.width, 1920);
        assert_eq!(decoded.height, 1080);
        assert_eq!(decoded.pixelformat, V4L2_PIX_FMT_RGB24);
        assert_eq!(decoded.colorspace, V4L2_COLORSPACE_SRGB);
        assert_eq!(decoded.bytesperline, 5760);
        assert_eq!(decoded.sizeimage, 6_220_800);
        assert_eq!(decoded.num_planes, 1);
        assert_eq!(format.raw[180], 1);
        assert_eq!(
            u32::from_ne_bytes(format.raw[24..28].try_into().unwrap()),
            5760
        );
        // Single-plane bytesperline is the multiplanar colorspace field.
        assert_eq!(format.pix().bytesperline, V4L2_COLORSPACE_SRGB);
        assert_ne!(format.pix().bytesperline, 5760);
    }

    #[test]
    fn multiplanar_buffer_stores_the_plane_pointer() {
        let mut plane: V4l2Plane = zeroed();
        plane.m.mem_offset = 4096;
        let mut buffer: V4l2Buffer = zeroed();
        let ptr = &mut plane as *mut V4l2Plane;
        buffer.set_planes(ptr);
        assert_eq!(unsafe { buffer.m.planes }, ptr);
        assert_eq!(plane.mmap_offset(), 4096);
    }

    #[test]
    fn fourcc_label_prints_rgb3_and_hides_non_text() {
        assert_eq!(fourcc_label(V4L2_PIX_FMT_RGB24), "RGB3:0x33424752");
        assert_eq!(fourcc_label(V4L2_PIX_FMT_BGR24), "BGR3:0x33524742");
        assert_eq!(fourcc_label(0), "0x00000000");
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
