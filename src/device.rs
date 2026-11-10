use std::io::Read;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use crate::error::{err, Result};
use crate::graph::{Entity, Graph, Link};
use crate::linux::{
    self, c_string, ioctl, zeroed, BtTimings, Capability, DvTimings, MediaEntityDesc,
    MediaLinkDesc, MediaLinksEnum, MediaPadDesc, RequestBuffers, SubdevFormat, V4l2Buffer,
    V4l2Edid, V4l2Format, MEDIA_BUS_FMT_RGB888_1X24, MEDIA_ENT_ID_FLAG_NEXT,
    MEDIA_IOC_ENUM_ENTITIES, MEDIA_IOC_ENUM_LINKS, MEDIA_IOC_SETUP_LINK, MEDIA_LNK_FL_ENABLED,
    MEDIA_LNK_FL_IMMUTABLE, MEDIA_LNK_FL_LINK_TYPE, V4L2_BUF_TYPE_VIDEO_CAPTURE,
    V4L2_CAP_DEVICE_CAPS, V4L2_COLORSPACE_SRGB, V4L2_FIELD_NONE, V4L2_MEMORY_MMAP,
    V4L2_PIX_FMT_RGB24, V4L2_SUBDEV_FORMAT_ACTIVE, VIDIOC_DQBUF, VIDIOC_G_FMT, VIDIOC_QBUF,
    VIDIOC_QUERYBUF, VIDIOC_QUERYCAP, VIDIOC_QUERY_DV_TIMINGS, VIDIOC_REQBUFS, VIDIOC_STREAMOFF,
    VIDIOC_STREAMON, VIDIOC_SUBDEV_G_FMT, VIDIOC_SUBDEV_S_FMT, VIDIOC_S_DV_TIMINGS, VIDIOC_S_EDID,
    VIDIOC_S_FMT,
};

const MAX_ENTITIES: usize = 256;
const MAX_PADS: u16 = 64;
const MAX_LINKS: u16 = 256;
const MAX_CAPTURE_BUFFERS: u32 = 8;
const MAX_CAPTURE_BYTES: usize = 16 * 1024 * 1024;

pub struct Opened {
    pub file: std::fs::File,
}

impl Opened {
    pub fn rw(path: &Path) -> Result<Self> {
        crate::nodes::open_device(path).map(|file| Self { file })
    }

    fn fd(&self) -> i32 {
        self.file.as_raw_fd()
    }
}

pub fn list_media_nodes(dir: &Path, only: Option<&Path>) -> Result<Vec<PathBuf>> {
    if let Some(path) = only {
        let resolved = crate::nodes::resolve_dev_node(path)?;
        if crate::nodes::dev_node_kind(&resolved) != Some("media") {
            return Err(err(format!(
                "--media must be a /dev/mediaN device, not {}.",
                resolved.display()
            )));
        }
        return Ok(vec![resolved]);
    }
    if dir != Path::new("/dev") {
        return Err(err("MEDIA_DIR must be /dev."));
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(err(format!(
                "No media device was found in {}. Load the tc358743 overlay, then reboot.",
                dir.display()
            )));
        }
        Err(error) => {
            return Err(err(format!("Cannot read {}: {error}", dir.display())));
        }
    };
    let mut nodes = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(suffix) = name.strip_prefix("media") else {
            continue;
        };
        if !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()) {
            let resolved = crate::nodes::resolve_dev_node(&entry.path())?;
            nodes.push((suffix.parse::<u32>().unwrap_or(u32::MAX), resolved));
        }
    }
    nodes.sort_by_key(|(index, _)| *index);
    Ok(nodes.into_iter().map(|(_, path)| path).collect())
}

pub fn read_graph(path: &Path) -> Result<Graph> {
    let opened = Opened::rw(path)?;
    let fd = opened.fd();
    let mut entities = Vec::new();
    let mut desc: MediaEntityDesc = zeroed();
    desc.id = MEDIA_ENT_ID_FLAG_NEXT;
    loop {
        if entities.len() >= MAX_ENTITIES {
            return Err(err("The media graph has more entities than expected."));
        }
        match linux::ioctl_raw(fd, MEDIA_IOC_ENUM_ENTITIES, &mut desc) {
            Ok(()) => {}
            Err(error) if error.raw_os_error() == Some(libc::EINVAL) && !entities.is_empty() => {
                break
            }
            Err(error) => {
                return Err(err(format!("enumerate media entities: {error}")));
            }
        }
        let id = desc.id & !MEDIA_ENT_ID_FLAG_NEXT;
        let name = c_string(&desc.name);
        let device = devnode(desc.major, desc.minor);
        let pads = desc.pads;
        let links = desc.links;
        if pads > MAX_PADS || links > MAX_LINKS {
            return Err(err(format!(
                "Entity {id} reported {pads} pads and {links} links."
            )));
        }
        entities.push((id, name, device, pads, links));
        desc.id |= MEDIA_ENT_ID_FLAG_NEXT;
    }

    let mut graph = Graph {
        entities: entities
            .iter()
            .map(|(id, name, device, _, _)| Entity {
                id: *id,
                name: name.clone(),
                device: device.clone(),
            })
            .collect(),
        links: Vec::new(),
    };

    for (id, _name, _device, pads, links) in &entities {
        if *pads == 0 && *links == 0 {
            continue;
        }
        let mut pad_buf = vec![zeroed::<MediaPadDesc>(); *pads as usize];
        let mut link_buf = vec![zeroed::<MediaLinkDesc>(); *links as usize];
        let mut request = MediaLinksEnum {
            entity: *id,
            _pad: 0,
            pads: if pad_buf.is_empty() {
                std::ptr::null_mut()
            } else {
                pad_buf.as_mut_ptr()
            },
            links: if link_buf.is_empty() {
                std::ptr::null_mut()
            } else {
                link_buf.as_mut_ptr()
            },
            reserved: [0; 4],
        };
        ioctl(
            fd,
            MEDIA_IOC_ENUM_LINKS,
            &mut request,
            &format!("enumerate links for entity {id}"),
        )?;
        for link in &link_buf {
            if link.flags & MEDIA_LNK_FL_LINK_TYPE != 0 {
                continue;
            }
            let Some(from_name) = name_of(&graph, link.source.entity) else {
                continue;
            };
            let Some(to_name) = name_of(&graph, link.sink.entity) else {
                continue;
            };
            let item = Link {
                from_id: link.source.entity,
                from_name,
                from_pad: u32::from(link.source.index),
                to_id: link.sink.entity,
                to_name,
                to_pad: u32::from(link.sink.index),
                flags: link.flags,
            };
            if !graph.links.iter().any(|existing| {
                existing.from_id == item.from_id
                    && existing.from_pad == item.from_pad
                    && existing.to_id == item.to_id
                    && existing.to_pad == item.to_pad
            }) {
                graph.links.push(item);
            }
        }
    }
    Ok(graph)
}

fn name_of(graph: &Graph, id: u32) -> Option<String> {
    graph
        .entities
        .iter()
        .find(|entity| entity.id == id)
        .map(|entity| entity.name.clone())
}

fn devnode(major: u32, minor: u32) -> Option<String> {
    if major == 0 {
        return None;
    }
    let path = format!("/sys/dev/char/{major}:{minor}/uevent");
    let file = std::fs::File::open(path).ok()?;
    let mut text = String::new();
    std::io::Read::take(file, 4096)
        .read_to_string(&mut text)
        .ok()?;
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("DEVNAME=") {
            return crate::nodes::devname_path(name);
        }
    }
    None
}

pub fn set_edid(node: &Path, bytes: &mut [u8]) -> Result<()> {
    if !bytes.len().is_multiple_of(128) || bytes.is_empty() || bytes.len() > 512 {
        return Err(err(
            "EDID length must be a multiple of 128, up to 512 bytes",
        ));
    }
    let opened = Opened::rw(node)?;
    let mut request = V4l2Edid {
        pad: 0,
        start_block: 0,
        blocks: (bytes.len() / 128) as u32,
        reserved: [0; 5],
        edid: bytes.as_mut_ptr(),
    };
    ioctl(
        opened.fd(),
        VIDIOC_S_EDID,
        &mut request,
        &format!("set EDID on {}", node.display()),
    )
}

pub fn query_timings(node: &Path) -> Result<DvTimings> {
    let opened = Opened::rw(node)?;
    let mut timings: DvTimings = zeroed();
    ioctl(
        opened.fd(),
        VIDIOC_QUERY_DV_TIMINGS,
        &mut timings,
        &format!("query HDMI timings on {}", node.display()),
    )?;
    Ok(timings)
}

pub fn set_timings(node: &Path, timings: &DvTimings) -> Result<()> {
    let opened = Opened::rw(node)?;
    let mut timings = *timings;
    ioctl(
        opened.fd(),
        VIDIOC_S_DV_TIMINGS,
        &mut timings,
        &format!("set HDMI timings on {}", node.display()),
    )
}

pub fn describe_timings(timings: &DvTimings) -> String {
    let bt = timings.bt();
    let width = bt.width;
    let height = bt.height;
    let pixelclock = bt.pixelclock;
    let hfront = bt.hfrontporch;
    let hsync = bt.hsync;
    let hback = bt.hbackporch;
    let vfront = bt.vfrontporch;
    let vsync = bt.vsync;
    let vback = bt.vbackporch;
    let interlaced = bt.interlaced;
    format!(
        "active {width}x{height}, pixel clock {pixelclock} Hz, horizontal {hfront}/{hsync}/{hback}, vertical {vfront}/{vsync}/{vback}, interlaced {}",
        if interlaced == 0 { "no" } else { "yes" }
    )
}

pub fn timings_active(timings: &DvTimings) -> Option<(u32, u32)> {
    let bt: BtTimings = timings.bt();
    if bt.width > 0 && bt.height > 0 {
        Some((bt.width, bt.height))
    } else {
        None
    }
}

pub fn reset_links(media: &Path, graph: &Graph) -> Result<()> {
    let opened = Opened::rw(media)?;
    for link in &graph.links {
        if link.flags & MEDIA_LNK_FL_ENABLED == 0 || link.flags & MEDIA_LNK_FL_IMMUTABLE != 0 {
            continue;
        }
        setup_link(&opened, link, link.flags & !MEDIA_LNK_FL_ENABLED)?;
    }
    Ok(())
}

pub fn enable_route_link(media: &Path, graph: &Graph, route: &crate::graph::Route) -> Result<()> {
    let opened = Opened::rw(media)?;
    let Some(link) = graph.links.iter().find(|link| {
        link.from_id == route.csi_id
            && link.from_pad == route.source_pad
            && link.to_id == route.video_id
            && link.to_pad == route.video_pad
    }) else {
        return Err(err(format!(
            "Cannot find the link {}:{} -> {}:{}",
            route.csi_name, route.source_pad, route.video_name, route.video_pad
        )));
    };
    setup_link(&opened, link, link.flags | MEDIA_LNK_FL_ENABLED)
}

fn setup_link(opened: &Opened, link: &Link, flags: u32) -> Result<()> {
    let mut desc = MediaLinkDesc {
        source: MediaPadDesc {
            entity: link.from_id,
            index: u16::try_from(link.from_pad)
                .map_err(|_| err("The source pad does not fit in 16 bits."))?,
            _pad: 0,
            flags: 0,
            reserved: [0; 2],
        },
        sink: MediaPadDesc {
            entity: link.to_id,
            index: u16::try_from(link.to_pad)
                .map_err(|_| err("The sink pad does not fit in 16 bits."))?,
            _pad: 0,
            flags: 0,
            reserved: [0; 2],
        },
        flags,
        reserved: [0; 2],
    };
    ioctl(
        opened.fd(),
        MEDIA_IOC_SETUP_LINK,
        &mut desc,
        &format!(
            "set link {}:{} -> {}:{}",
            link.from_name, link.from_pad, link.to_name, link.to_pad
        ),
    )
}

pub fn set_subdev_format(node: &Path, pad: u32, width: u32, height: u32) -> Result<SubdevFormat> {
    let opened = Opened::rw(node)?;
    let mut format = subdev_format(pad, width, height);
    ioctl(
        opened.fd(),
        VIDIOC_SUBDEV_S_FMT,
        &mut format,
        &format!("set format on {} pad {pad}", node.display()),
    )?;
    Ok(format)
}

pub fn get_subdev_format(node: &Path, pad: u32) -> Result<SubdevFormat> {
    let opened = Opened::rw(node)?;
    let mut format: SubdevFormat = zeroed();
    format.which = V4L2_SUBDEV_FORMAT_ACTIVE;
    format.pad = pad;
    ioctl(
        opened.fd(),
        VIDIOC_SUBDEV_G_FMT,
        &mut format,
        &format!("read format on {} pad {pad}", node.display()),
    )?;
    Ok(format)
}

fn subdev_format(pad: u32, width: u32, height: u32) -> SubdevFormat {
    let mut format: SubdevFormat = zeroed();
    format.which = V4L2_SUBDEV_FORMAT_ACTIVE;
    format.pad = pad;
    format.width = width;
    format.height = height;
    format.code = MEDIA_BUS_FMT_RGB888_1X24;
    format.field = V4L2_FIELD_NONE;
    format.colorspace = V4L2_COLORSPACE_SRGB;
    format
}

pub fn format_bus(format: &SubdevFormat) -> String {
    let name = if format.code == MEDIA_BUS_FMT_RGB888_1X24 {
        "RGB888_1X24"
    } else {
        "code"
    };
    format!(
        "{name} {}x{} field={} colorspace={} code=0x{:04x}",
        format.width,
        format.height,
        field_name(format.field),
        colorspace_name(format.colorspace),
        format.code
    )
}

pub fn set_video_format(node: &Path, width: u32, height: u32) -> Result<linux::PixFormat> {
    let opened = Opened::rw(node)?;
    let mut format: V4l2Format = zeroed();
    format.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
    let pix = linux::PixFormat {
        width,
        height,
        pixelformat: V4L2_PIX_FMT_RGB24,
        field: V4L2_FIELD_NONE,
        bytesperline: 0,
        sizeimage: 0,
        colorspace: V4L2_COLORSPACE_SRGB,
        priv_: 0,
        flags: 0,
        ycbcr_enc: 0,
        quantization: 0,
        xfer_func: 0,
    };
    format.set_pix(&pix);
    ioctl(
        opened.fd(),
        VIDIOC_S_FMT,
        &mut format,
        &format!("set video format on {}", node.display()),
    )?;
    Ok(format.pix())
}

pub fn get_video_format(node: &Path) -> Result<linux::PixFormat> {
    let opened = Opened::rw(node)?;
    let mut format: V4l2Format = zeroed();
    format.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
    ioctl(
        opened.fd(),
        VIDIOC_G_FMT,
        &mut format,
        &format!("read video format on {}", node.display()),
    )?;
    Ok(format.pix())
}

pub fn query_caps(node: &Path) -> Result<String> {
    let opened = Opened::rw(node)?;
    let mut caps: Capability = zeroed();
    ioctl(
        opened.fd(),
        VIDIOC_QUERYCAP,
        &mut caps,
        &format!("query capabilities of {}", node.display()),
    )?;
    let bits = if caps.capabilities & V4L2_CAP_DEVICE_CAPS != 0 {
        caps.device_caps
    } else {
        caps.capabilities
    };
    let mut names = Vec::new();
    if bits & linux::V4L2_CAP_VIDEO_CAPTURE != 0 {
        names.push("capture");
    }
    if bits & linux::V4L2_CAP_VIDEO_CAPTURE_MPLANE != 0 {
        names.push("capture-mplane");
    }
    if names.is_empty() {
        names.push("other");
    }
    Ok(format!("{} caps=0x{bits:08x}", names.join(",")))
}

struct CaptureSession {
    fd: i32,
    maps: Vec<(*mut libc::c_void, usize)>,
    streaming: bool,
    buf_type: libc::c_int,
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        if self.streaming {
            let _ = ioctl(
                self.fd,
                VIDIOC_STREAMOFF,
                &mut self.buf_type,
                "stop capture",
            );
        }
        for (ptr, len) in self.maps.drain(..) {
            unsafe {
                libc::munmap(ptr, len);
            }
        }
        let mut release: RequestBuffers = zeroed();
        release.count = 0;
        release.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
        release.memory = V4L2_MEMORY_MMAP;
        let _ = ioctl(
            self.fd,
            VIDIOC_REQBUFS,
            &mut release,
            "release capture buffers",
        );
    }
}

pub fn capture_frames(node: &Path, output: &Path, skip: u32, count: u32) -> Result<u64> {
    let opened = Opened::rw(node)?;
    let fd = opened.fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags >= 0 {
        unsafe {
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }
    let mut request: RequestBuffers = zeroed();
    request.count = 4;
    request.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
    request.memory = V4L2_MEMORY_MMAP;
    ioctl(fd, VIDIOC_REQBUFS, &mut request, "request capture buffers")?;
    if request.count < 2 || request.count > MAX_CAPTURE_BUFFERS {
        return Err(err(format!(
            "The driver offered {} capture buffers.",
            request.count
        )));
    }

    let mut session = CaptureSession {
        fd,
        maps: Vec::new(),
        streaming: false,
        buf_type: V4L2_BUF_TYPE_VIDEO_CAPTURE as libc::c_int,
    };
    for index in 0..request.count {
        let mut buffer: V4l2Buffer = zeroed();
        buffer.index = index;
        buffer.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
        buffer.memory = V4L2_MEMORY_MMAP;
        ioctl(fd, VIDIOC_QUERYBUF, &mut buffer, "query capture buffer")?;
        let length = buffer.length as usize;
        if length == 0 || length > MAX_CAPTURE_BYTES {
            return Err(err(format!("Capture buffer {index} is {length} bytes.")));
        }
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                buffer.offset as libc::off_t,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(err(format!(
                "mmap of capture buffer {index} failed: {}",
                std::io::Error::last_os_error()
            )));
        }
        session.maps.push((ptr, length));
        ioctl(fd, VIDIOC_QBUF, &mut buffer, "queue capture buffer")?;
    }

    ioctl(fd, VIDIOC_STREAMON, &mut session.buf_type, "start capture")?;
    session.streaming = true;
    let mut file = crate::nodes::create_output(output)?;
    let mut written = 0u64;
    let mut kept = 0u32;
    let mut seen = 0u32;
    while kept < count {
        wait_for_frame(fd)?;
        let mut buffer: V4l2Buffer = zeroed();
        buffer.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
        buffer.memory = V4L2_MEMORY_MMAP;
        ioctl(fd, VIDIOC_DQBUF, &mut buffer, "dequeue capture buffer")?;
        let index = buffer.index as usize;
        let Some((ptr, len)) = session.maps.get(index) else {
            return Err(err(format!(
                "The driver returned capture buffer {index}, which was not queued."
            )));
        };
        let used = if buffer.bytesused == 0 {
            *len
        } else {
            (buffer.bytesused as usize).min(*len)
        };
        if seen >= skip {
            let slice = unsafe { std::slice::from_raw_parts(*ptr as *const u8, used) };
            std::io::Write::write_all(&mut file, slice)?;
            written += used as u64;
            kept += 1;
        }
        seen += 1;
        ioctl(fd, VIDIOC_QBUF, &mut buffer, "requeue capture buffer")?;
        if seen > skip + count + 20 {
            return Err(err("The frame test stopped making progress."));
        }
    }
    Ok(written)
}

fn wait_for_frame(fd: i32) -> Result<()> {
    let mut pollfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    loop {
        let rc = unsafe { libc::poll(&mut pollfd, 1, 5_000) };
        if rc < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err(format!("Waiting for a frame failed: {error}")));
        }
        if rc == 0 {
            return Err(err("No frames arrived within 5 seconds."));
        }
        return Ok(());
    }
}

fn field_name(field: u32) -> &'static str {
    if field == V4L2_FIELD_NONE {
        "none"
    } else {
        "other"
    }
}

fn colorspace_name(colorspace: u32) -> &'static str {
    if colorspace == V4L2_COLORSPACE_SRGB {
        "srgb"
    } else {
        "other"
    }
}
