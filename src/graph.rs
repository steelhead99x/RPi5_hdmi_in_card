use crate::error::{err, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entity {
    pub id: u32,
    pub name: String,
    pub device: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub from_id: u32,
    pub from_name: String,
    pub from_pad: u32,
    pub to_id: u32,
    pub to_name: String,
    pub to_pad: u32,
    pub flags: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Graph {
    pub entities: Vec<Entity>,
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub bridge_name: String,
    pub bridge_dev: String,
    pub bridge_id: u32,
    pub csi_name: String,
    pub csi_dev: String,
    pub csi_id: u32,
    pub source_pad: u32,
    pub video_name: String,
    pub video_dev: String,
    pub video_id: u32,
    pub video_pad: u32,
}

impl Graph {
    pub fn bridges(&self) -> Vec<&Entity> {
        self.entities
            .iter()
            .filter(|entity| {
                entity.name.starts_with("tc358743") && device_is_node(entity.device.as_deref())
            })
            .collect()
    }

    pub fn entity_lines(&self) -> String {
        if self.entities.is_empty() {
            return "  (none)".to_string();
        }
        self.entities
            .iter()
            .map(|entity| {
                let node = entity.device.as_deref().unwrap_or("-");
                format!("  {} {node}", entity.name)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn select_route(graph: &Graph) -> Result<Route> {
    let Some(bridge) = graph.bridges().into_iter().next() else {
        return Err(err(format!(
            "The media graph has no TC358743 subdevice.\nEntities:\n{}",
            graph.entity_lines()
        )));
    };
    let Some(csi) = graph.entities.iter().find(|entity| entity.name == "csi2") else {
        return Err(err(format!(
            "The media graph has no csi2 entity.\nEntities:\n{}",
            graph.entity_lines()
        )));
    };
    let Some(csi_dev) = csi
        .device
        .clone()
        .filter(|device| device.starts_with("/dev/"))
    else {
        return Err(err("The csi2 entity has no device node."));
    };

    let mut fallback: Option<&Link> = None;
    let mut preferred: Option<&Link> = None;
    for link in &graph.links {
        if link.from_name != "csi2" {
            continue;
        }
        let Some(target) = graph
            .entities
            .iter()
            .find(|entity| entity.name == link.to_name)
        else {
            continue;
        };
        let Some(device) = target.device.as_deref() else {
            continue;
        };
        if !device.starts_with("/dev/video") {
            continue;
        }
        if link.to_name.contains("ch0") {
            preferred = Some(link);
            break;
        }
        if fallback.is_none() {
            fallback = Some(link);
        }
    }
    let Some(link) = preferred.or(fallback) else {
        return Err(err(format!(
            "The media graph has no video node linked from csi2.\nEntities:\n{}",
            graph.entity_lines()
        )));
    };
    Ok(Route {
        bridge_name: bridge.name.clone(),
        bridge_dev: bridge.device.clone().unwrap(),
        bridge_id: bridge.id,
        csi_name: csi.name.clone(),
        csi_dev,
        csi_id: csi.id,
        source_pad: link.from_pad,
        video_name: link.to_name.clone(),
        video_dev: graph
            .entities
            .iter()
            .find(|entity| entity.name == link.to_name)
            .and_then(|entity| entity.device.clone())
            .unwrap(),
        video_id: link.to_id,
        video_pad: link.to_pad,
    })
}

fn device_is_node(device: Option<&str>) -> bool {
    device.is_some_and(|value| value.starts_with("/dev/"))
}

/// Parse `media-ctl -p` text. The live tool reads the kernel graph with ioctls.
/// This parser exists so tests can use a captured text fixture.
#[cfg(test)]
pub fn parse_media_ctl(text: &str) -> Result<Graph> {
    let mut graph = Graph::default();
    let mut current: Option<String> = None;
    let mut current_pad: Option<u32> = None;
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("- entity ") {
            let Some((_, after_id)) = rest.split_once(':') else {
                continue;
            };
            let after_id = after_id.trim();
            let Some((name, _)) = after_id.split_once(" (") else {
                continue;
            };
            current = Some(name.to_string());
            current_pad = None;
            graph.entities.push(Entity {
                id: 0,
                name: name.to_string(),
                device: None,
            });
            continue;
        }
        if let Some(name) = current.clone() {
            if let Some(after) = line.split("device node name ").nth(1) {
                if let Some(device) = after.split_whitespace().next() {
                    if let Some(entity) =
                        graph.entities.iter_mut().find(|entity| entity.name == name)
                    {
                        entity.device = Some(device.to_string());
                    }
                }
                continue;
            }
        }
        if let Some(rest) = trimmed.strip_prefix("pad") {
            if let Some((number, _)) = rest.split_once(':') {
                if !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()) {
                    current_pad = number.parse().ok();
                }
            }
        }
        if let Some(name) = current.clone() {
            if let Some(pad) = current_pad {
                if let Some(link) = parse_forward_link(line, &name, pad) {
                    graph.links.push(link);
                }
            }
        }
    }
    Ok(graph)
}

#[cfg(test)]
fn parse_forward_link(line: &str, from_name: &str, from_pad: u32) -> Option<Link> {
    let marker = "-> \"";
    let start = line.find(marker)?;
    let after = &line[start + marker.len()..];
    let (to_name, rest) = after.split_once('"')?;
    let rest = rest.trim().strip_prefix(':')?.trim();
    let pad_text: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    if pad_text.is_empty() {
        return None;
    }
    let to_pad = pad_text.parse().ok()?;
    Some(Link {
        from_id: 0,
        from_name: from_name.to_string(),
        from_pad,
        to_id: 0,
        to_name: to_name.to_string(),
        to_pad,
        flags: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/media-graph.txt");

    #[test]
    fn fixture_selects_the_image_channel_not_metadata() {
        let graph = parse_media_ctl(FIXTURE).unwrap();
        let route = select_route(&graph).unwrap();
        assert_eq!(route.bridge_name, "tc358743 11-000f");
        assert_eq!(route.bridge_dev, "/dev/v4l-subdev7");
        assert_eq!(route.csi_dev, "/dev/v4l-subdev0");
        assert_eq!(route.source_pad, 4);
        assert_eq!(route.video_name, "rp1-cfe-csi2_ch0");
        assert_eq!(route.video_dev, "/dev/video33");
        assert_eq!(route.video_pad, 0);
        assert_ne!(route.video_dev, "/dev/video34");
        assert_ne!(route.video_dev, "/dev/video40");
    }

    #[test]
    fn fallback_when_the_image_channel_name_has_no_ch0() {
        let text = "\
- entity 1: csi2 (8 pads, 2 links)
            device node name /dev/v4l-subdev0
	pad6: Source
		-> \"rp1-cfe-csi2_ch2\":0 []
- entity 2: tc358743 4-000f (1 pad, 1 link)
            device node name /dev/v4l-subdev4
	pad0: Source
		-> \"csi2\":0 [ENABLED,IMMUTABLE]
- entity 3: rp1-cfe-csi2_ch2 (1 pad, 1 link)
            device node name /dev/video8
	pad0: Sink
";
        let route = select_route(&parse_media_ctl(text).unwrap()).unwrap();
        assert_eq!(route.bridge_dev, "/dev/v4l-subdev4");
        assert_eq!(route.video_dev, "/dev/video8");
        assert_eq!(route.source_pad, 6);
    }

    #[test]
    fn missing_bridge_is_an_error() {
        let text =
            "- entity 1: csi2 (8 pads, 0 links)\n            device node name /dev/v4l-subdev0\n";
        let error = select_route(&parse_media_ctl(text).unwrap()).unwrap_err();
        assert!(error.to_string().contains("no TC358743"));
    }
}
