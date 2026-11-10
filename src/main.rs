mod device;
mod edid;
mod error;
mod graph;
mod linux;
mod nodes;
mod run;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::{Parser, Subcommand};

use crate::error::{err, Result};
use crate::run::SetupOptions;

#[derive(Parser, Debug)]
#[command(
    name = "hdmi-in",
    about = "Set up a TC358743 HDMI to CSI-2 input.",
    disable_help_subcommand = true
)]
struct Cli {
    /// Print the bridge, video node, link, bus format, and signal lock.
    #[command(subcommand)]
    command: Option<Command>,

    /// EDID name or file. Built-in names: 720p60edid, 1080p25edid, 1080p30edid, 1080p50edid, 1080p60edid.
    #[arg(long, env = "VIDEDID", default_value = "1080p60edid")]
    edid: String,

    /// EDID name or file, as a trailing argument.
    #[arg(value_name = "EDID")]
    edid_arg: Option<String>,

    /// Media device to use, such as /dev/media1.
    #[arg(long)]
    media: Option<PathBuf>,

    /// Picture width to use when the source reports no timings.
    #[arg(long)]
    width: Option<u32>,

    /// Picture height to use when the source reports no timings.
    #[arg(long)]
    height: Option<u32>,

    /// Frame-test file. Default: csitest.yuv in the current directory.
    #[arg(long, default_value = "csitest.yuv")]
    output: PathBuf,

    /// Configure the pipeline and do not capture test frames.
    #[arg(long)]
    no_test: bool,

    /// Print the planned changes and do not program the device.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Print the bridge, video node, link, bus format, and signal lock.
    Status {
        /// Media device to read, such as /dev/media1.
        #[arg(long)]
        media: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    match real_main() {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(error) => {
            eprintln!("hdmi-in: {error}");
            ExitCode::from(1)
        }
    }
}

fn real_main() -> Result<i32> {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let code = match error.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => 0,
                _ => 1,
            };
            error.print().ok();
            return Ok(code);
        }
    };
    if let Some(Command::Status { media }) = cli.command {
        let media_dir = media_dir();
        return run::status(media.as_deref(), &media_dir);
    }
    if cli.edid_arg.is_some()
        && std::env::args().any(|arg| arg == "--edid" || arg.starts_with("--edid="))
    {
        return Err(err(
            "Pass the EDID either with --edid or as one trailing argument, not both.",
        ));
    }
    let edid_name = cli.edid_arg.unwrap_or(cli.edid);
    let options = SetupOptions {
        edid_name,
        media: cli.media,
        media_dir: media_dir(),
        width: cli.width,
        height: cli.height,
        output: cli.output,
        test: !cli.no_test,
        dry_run: cli.dry_run,
        sleep_seconds: sleep_seconds()?,
    };
    run::setup(&options)
}

fn media_dir() -> PathBuf {
    std::env::var_os("MEDIA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/dev"))
}

fn sleep_seconds() -> Result<u64> {
    match std::env::var("SLEEP_SECONDS") {
        Ok(value) => value
            .parse::<u64>()
            .map_err(|_| err("SLEEP_SECONDS must be a whole number of seconds.")),
        Err(_) => Ok(5),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, FromArgMatches};

    #[test]
    fn setup_flags_parse() {
        let matches = Cli::command()
            .try_get_matches_from([
                "hdmi-in",
                "--edid",
                "720p60edid",
                "--media",
                "/dev/media1",
                "--no-test",
                "--dry-run",
                "--width",
                "1280",
                "--height",
                "720",
                "--output",
                "out.yuv",
            ])
            .unwrap();
        let cli = Cli::from_arg_matches(&matches).unwrap();
        assert!(cli.command.is_none());
        assert_eq!(cli.edid, "720p60edid");
        assert!(cli.no_test);
        assert!(cli.dry_run);
        assert_eq!(cli.width, Some(1280));
    }

    #[test]
    fn status_subcommand_parses() {
        let matches = Cli::command()
            .try_get_matches_from(["hdmi-in", "status", "--media", "/dev/media2"])
            .unwrap();
        let cli = Cli::from_arg_matches(&matches).unwrap();
        match cli.command {
            Some(Command::Status { media }) => {
                assert_eq!(media.unwrap(), PathBuf::from("/dev/media2"))
            }
            None => panic!("status subcommand was dropped"),
        }
    }

    #[test]
    fn help_text_stays_plain() {
        let mut help = Vec::new();
        Cli::command().write_long_help(&mut help).unwrap();
        let text = String::from_utf8(help).unwrap();
        assert!(text.contains("--edid"));
        assert!(text.contains("--no-test"));
        assert!(text.contains("--dry-run"));
        assert!(text.contains("status"));
        assert!(!text.contains('\u{2014}'));
        assert!(!text.contains(" AI"));
        assert!(!text.contains(" A.I."));
    }
}
