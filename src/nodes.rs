use std::fs::OpenOptions;
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use crate::error::{err, Result};

/// Classify a direct `/dev` node name. Paths with `..` or extra components do not match.
pub fn dev_node_kind(path: &Path) -> Option<&'static str> {
    let text = path.to_str()?;
    let name = text.strip_prefix("/dev/")?;
    if name.contains('/') || name.contains('.') {
        return None;
    }
    let (kind, digits) = if let Some(rest) = name.strip_prefix("v4l-subdev") {
        ("v4l-subdev", rest)
    } else if let Some(rest) = name.strip_prefix("media") {
        ("media", rest)
    } else {
        let rest = name.strip_prefix("video")?;
        ("video", rest)
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(kind)
}

/// Turn a user path into a `/dev` node. A symlink is kept only when its target
/// is also a direct node under `/dev`.
pub fn resolve_dev_node(path: &Path) -> Result<PathBuf> {
    let meta = std::fs::symlink_metadata(path)
        .map_err(|error| err(format!("Cannot inspect {}: {error}", path.display())))?;
    if meta.file_type().is_symlink() {
        let target = std::fs::canonicalize(path)
            .map_err(|error| err(format!("Cannot resolve {}: {error}", path.display())))?;
        if dev_node_kind(&target).is_none() {
            return Err(err(format!(
                "Refusing {} because it points outside /dev.",
                path.display()
            )));
        }
        let target_meta = std::fs::symlink_metadata(&target)
            .map_err(|error| err(format!("Cannot inspect {}: {error}", target.display())))?;
        if target_meta.file_type().is_symlink() {
            return Err(err(format!(
                "Refusing {} because {} is also a symlink.",
                path.display(),
                target.display()
            )));
        }
        return Ok(target);
    }
    if dev_node_kind(path).is_none() {
        return Err(err(format!(
            "Refusing {}. Expected /dev/mediaN, /dev/videoN, or /dev/v4l-subdevN.",
            path.display()
        )));
    }
    Ok(path.to_path_buf())
}

pub fn open_device(path: &Path) -> Result<std::fs::File> {
    let resolved = resolve_dev_node(path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let file = options.open(&resolved).map_err(|error| {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            err(format!(
                "Cannot open {}: {error}. The user needs access to video devices.",
                resolved.display()
            ))
        } else {
            err(format!("Cannot open {}: {error}", resolved.display()))
        }
    })?;
    let meta = file
        .metadata()
        .map_err(|error| err(format!("Cannot inspect {}: {error}", resolved.display())))?;
    if !meta.file_type().is_char_device() {
        return Err(err(format!(
            "{} is not a character device.",
            resolved.display()
        )));
    }
    Ok(file)
}

pub fn create_output(path: &Path) -> Result<std::fs::File> {
    if path == Path::new("/dev/null") {
        return OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_CLOEXEC)
            .open(path)
            .map_err(|error| err(format!("Cannot open /dev/null: {error}")));
    }
    if path.starts_with("/dev") {
        return Err(err("Refusing to write the frame test under /dev."));
    }
    if path.as_os_str().is_empty() {
        return Err(err("The frame test path is empty."));
    }
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    options
        .open(path)
        .map_err(|error| err(format!("Cannot create {}: {error}", path.display())))
}

pub fn devname_path(name: &str) -> Option<String> {
    let relative = name.strip_prefix("/dev/").unwrap_or(name);
    if relative.contains('/') || relative.contains('.') || relative.contains('\0') {
        return None;
    }
    let path = format!("/dev/{relative}");
    dev_node_kind(Path::new(&path)).map(|_| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_direct_dev_nodes() {
        assert_eq!(dev_node_kind(Path::new("/dev/media1")), Some("media"));
        assert_eq!(dev_node_kind(Path::new("/dev/video33")), Some("video"));
        assert_eq!(
            dev_node_kind(Path::new("/dev/v4l-subdev7")),
            Some("v4l-subdev")
        );
        assert_eq!(devname_path("video33").as_deref(), Some("/dev/video33"));
        assert_eq!(
            devname_path("/dev/v4l-subdev0").as_deref(),
            Some("/dev/v4l-subdev0")
        );
    }

    #[test]
    fn rejects_paths_that_leave_dev() {
        for path in [
            "/tmp/media0",
            "/dev/../tmp/media0",
            "/dev/media0/extra",
            "/dev/media",
            "media0",
            "/dev/video33/../../etc/passwd",
            "/etc/passwd",
        ] {
            assert_eq!(dev_node_kind(Path::new(path)), None, "{path}");
        }
        assert_eq!(devname_path("../../etc/passwd"), None);
        assert_eq!(devname_path("video33/extra"), None);
        assert_eq!(devname_path("/tmp/video0"), None);
    }

    #[test]
    fn frame_output_refuses_dev_and_a_symlink() {
        let error = create_output(Path::new("/dev/video0")).unwrap_err();
        assert!(error.to_string().contains("/dev"));
        let dir = std::env::temp_dir().join(format!(
            "hdmi-in-out-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("frames.yuv");
        let link = dir.join("link.yuv");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let error = create_output(&link).unwrap_err();
        assert!(error.to_string().contains("link.yuv"), "{error}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
