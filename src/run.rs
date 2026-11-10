use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::device::{self, describe_timings, timings_active};
use crate::edid::{self, Edid};
use crate::error::{err, Result};
use crate::graph::{select_route, Graph, Route};

#[derive(Debug, Clone)]
pub struct SetupOptions {
    pub edid_name: String,
    pub media: Option<PathBuf>,
    pub media_dir: PathBuf,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub output: PathBuf,
    pub test: bool,
    pub dry_run: bool,
    pub sleep_seconds: u64,
}

pub fn setup(options: &SetupOptions) -> Result<i32> {
    if options.width.is_some() != options.height.is_some() {
        return Err(err("Pass both --width and --height."));
    }
    if let Some(width) = options.width {
        let height = options.height.unwrap_or(0);
        if !(1..=99_999).contains(&width) || !(1..=99_999).contains(&height) {
            return Err(err(
                "--width and --height must be positive numbers up to 99999.",
            ));
        }
    }
    let edid = edid::load(&options.edid_name)?;
    let (media, graph, route) = find_route(options.media.as_deref(), &options.media_dir)?;
    println!("Media device: {}", media.display());
    println!("Bridge: {} ({})", route.bridge_name, route.bridge_dev);
    println!("Video node: {}", route.video_dev);
    let (preferred_w, preferred_h, preferred_hz) = edid::preferred_timing(&edid.bytes)?;
    println!(
        "EDID: {} from {} ({} bytes, checksum ok, preferred {preferred_w}x{preferred_h} at {preferred_hz:.2} Hz)",
        edid.name,
        edid.source,
        edid.bytes.len()
    );
    if options.dry_run {
        println!("Dry-run does not load the EDID, so any timings below are the current signal.");
    } else {
        let mut bytes = edid.bytes.clone();
        device::set_edid(Path::new(&route.bridge_dev), &mut bytes)?;
        if options.sleep_seconds > 0 {
            println!(
                "Waiting {} seconds for the source to read the EDID.",
                options.sleep_seconds
            );
            thread::sleep(Duration::from_secs(options.sleep_seconds));
        }
    }

    let timings = device::query_timings(Path::new(&route.bridge_dev));
    let active = match &timings {
        Ok(value) => {
            println!("{}", describe_timings(value));
            timings_active(value)
        }
        Err(error) => {
            eprintln!("hdmi-in: {error}");
            None
        }
    };

    let (width, height, have_signal) = match active {
        Some((width, height)) => {
            if let Some((nominal_w, nominal_h)) = edid::nominal_size(&edid.name) {
                if width != nominal_w || height != nominal_h {
                    eprintln!(
                        "hdmi-in: the source reported {width}x{height}. The selected EDID prefers {nominal_w}x{nominal_h}."
                    );
                }
            }
            (width, height, true)
        }
        None => {
            let (width, height) = fallback_size(options, &edid)?;
            eprintln!(
                "hdmi-in: no HDMI timings were detected. The pipeline will use {width}x{height}."
            );
            (width, height, false)
        }
    };
    println!("Picture: {width}x{height} RGB");

    if options.dry_run {
        println!(
            "Would enable link {}:{} -> {}:{}",
            route.csi_name, route.source_pad, route.video_name, route.video_pad
        );
        println!(
            "Would set RGB888_1X24 {width}x{height} on {}, {} pad 0, and {} pad {}.",
            route.bridge_dev, route.csi_dev, route.csi_dev, route.source_pad
        );
        println!("Would set RGB3 {width}x{height} on {}.", route.video_dev);
        if have_signal && options.test {
            println!("Would capture 2 frames to {}.", options.output.display());
        }
    } else {
        if have_signal {
            if let Ok(value) = timings {
                device::set_timings(Path::new(&route.bridge_dev), &value)?;
            }
        }
        device::reset_links(&media, &graph)?;
        device::enable_route_link(&media, &graph, &route)?;
        device::set_subdev_format(Path::new(&route.bridge_dev), 0, width, height)?;
        device::set_subdev_format(Path::new(&route.csi_dev), 0, width, height)?;
        device::set_subdev_format(Path::new(&route.csi_dev), route.source_pad, width, height)?;
        let programmed = device::set_video_format(Path::new(&route.video_dev), width, height)?;
        if programmed.pixelformat != crate::linux::V4L2_PIX_FMT_RGB24
            || programmed.width != width
            || programmed.height != height
        {
            eprintln!(
                "hdmi-in: the video node accepted {}x{} fourcc 0x{:08x}.",
                programmed.width, programmed.height, programmed.pixelformat
            );
        }
    }

    if !have_signal {
        if options.dry_run {
            eprintln!("hdmi-in: no HDMI timings were detected.");
        } else {
            eprintln!(
                "hdmi-in: the EDID is loaded. Connect the HDMI source and run this command again so the detected size can be applied."
            );
        }
        return Ok(2);
    }
    if !options.test || options.dry_run {
        return Ok(0);
    }
    let bytes = device::capture_frames(Path::new(&route.video_dev), &options.output, 3, 2)?;
    if options.output != Path::new("/dev/null") && bytes == 0 {
        return Err(err(format!(
            "The frame test wrote no data to {}.",
            options.output.display()
        )));
    }
    println!("Wrote {bytes} bytes to {}.", options.output.display());
    Ok(0)
}

pub fn status(media: Option<&Path>, media_dir: &Path) -> Result<i32> {
    let (path, graph, route) = find_route(media, media_dir)?;
    println!("media_device: {}", path.display());
    println!("bridge: {}", route.bridge_name);
    println!("bridge_node: {}", route.bridge_dev);
    println!("csi_entity: {}", route.csi_name);
    println!("csi_node: {}", route.csi_dev);
    println!(
        "link: {}:{} -> {}:{}",
        route.csi_name, route.source_pad, route.video_name, route.video_pad
    );
    if let Some(link) = graph.links.iter().find(|link| {
        link.from_id == route.csi_id
            && link.from_pad == route.source_pad
            && link.to_id == route.video_id
    }) {
        let enabled = if link.flags & crate::linux::MEDIA_LNK_FL_ENABLED != 0 {
            "enabled"
        } else {
            "disabled"
        };
        println!("link_flags: {enabled}");
    }
    println!("video_entity: {}", route.video_name);
    println!("video_node: {}", route.video_dev);
    match device::query_caps(Path::new(&route.video_dev)) {
        Ok(text) => println!("video_caps: {text}"),
        Err(error) => println!("video_caps: unavailable ({error})"),
    }
    print_bus("bridge_format", &route.bridge_dev, 0);
    print_bus("csi_sink_format", &route.csi_dev, 0);
    print_bus("csi_source_format", &route.csi_dev, route.source_pad);
    match device::get_video_format(Path::new(&route.video_dev)) {
        Ok(pix) => println!(
            "video_format: {}x{} fourcc=0x{:08x} bytesperline={} sizeimage={}",
            pix.width, pix.height, pix.pixelformat, pix.bytesperline, pix.sizeimage
        ),
        Err(error) => println!("video_format: unavailable ({error})"),
    }
    match device::query_timings(Path::new(&route.bridge_dev)) {
        Ok(timings) => {
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
            let locked = width > 0 && height > 0;
            println!("signal_lock: {}", if locked { "yes" } else { "no" });
            println!("active_width: {width}");
            println!("active_height: {height}");
            println!("pixel_clock_hz: {pixelclock}");
            println!("h_front: {hfront}");
            println!("h_sync: {hsync}");
            println!("h_back: {hback}");
            println!("v_front: {vfront}");
            println!("v_sync: {vsync}");
            println!("v_back: {vback}");
            println!("interlaced: {}", if interlaced == 0 { "no" } else { "yes" });
        }
        Err(error) => {
            println!("signal_lock: no");
            println!("timings_error: {error}");
        }
    }
    let _ = graph;
    Ok(0)
}

fn print_bus(label: &str, node: &str, pad: u32) {
    match device::get_subdev_format(Path::new(node), pad) {
        Ok(format) => println!("{label}: {}", device::format_bus(&format)),
        Err(error) => println!("{label}: unavailable ({error})"),
    }
}

fn fallback_size(options: &SetupOptions, edid: &Edid) -> Result<(u32, u32)> {
    if let (Some(width), Some(height)) = (options.width, options.height) {
        return Ok((width, height));
    }
    edid::nominal_size(&edid.name).ok_or_else(|| {
        err(
            "No HDMI timings were detected, and this EDID has no known picture size. Pass --width and --height.",
        )
    })
}

fn find_route(only: Option<&Path>, media_dir: &Path) -> Result<(PathBuf, Graph, Route)> {
    let nodes = device::list_media_nodes(media_dir, only)?;
    if nodes.is_empty() {
        return Err(err(format!(
            "No media device was found in {}. Load the tc358743 overlay, then reboot.",
            media_dir.display()
        )));
    }
    let mut found = Vec::new();
    let mut checked = Vec::new();
    for node in &nodes {
        checked.push(node.display().to_string());
        match device::read_graph(node) {
            Ok(graph) if !graph.bridges().is_empty() => found.push((node.clone(), graph)),
            Ok(_) => {}
            Err(error) => {
                if only.is_some() {
                    return Err(error);
                }
            }
        }
    }
    if found.is_empty() {
        return Err(err(format!(
            "No TC358743 bridge was found. If the card is on camera port 0, add cam0 to the overlay. Checked: {}",
            checked.join(", ")
        )));
    }
    if found.len() > 1 {
        let names = found
            .iter()
            .map(|(path, _)| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!(
            "hdmi-in: more than one TC358743 was found ({names}). Using {}. Pass --media to choose.",
            found[0].0.display()
        );
    }
    let (path, graph) = found.remove(0);
    let route = select_route(&graph)?;
    Ok((path, graph, route))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_edid_supplies_a_size_when_there_is_no_signal() {
        let edid = edid::load("1080p60edid").unwrap();
        let options = SetupOptions {
            edid_name: edid.name.clone(),
            media: None,
            media_dir: PathBuf::from("/dev"),
            width: None,
            height: None,
            output: PathBuf::from("csitest.yuv"),
            test: true,
            dry_run: false,
            sleep_seconds: 0,
        };
        assert_eq!(fallback_size(&options, &edid).unwrap(), (1920, 1080));
    }

    #[test]
    fn custom_edid_without_a_signal_needs_a_size() {
        let edid = Edid {
            name: "custom.edid".to_string(),
            source: "test".to_string(),
            bytes: Vec::new(),
        };
        let options = SetupOptions {
            edid_name: edid.name.clone(),
            media: None,
            media_dir: PathBuf::from("/dev"),
            width: None,
            height: None,
            output: PathBuf::from("csitest.yuv"),
            test: false,
            dry_run: true,
            sleep_seconds: 0,
        };
        assert!(fallback_size(&options, &edid).is_err());
        let mut with_size = options;
        with_size.width = Some(640);
        with_size.height = Some(480);
        assert_eq!(fallback_size(&with_size, &edid).unwrap(), (640, 480));
    }
}
