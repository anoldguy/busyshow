use std::fmt;
use std::path::{Path, PathBuf};

use busybar_anim::Target;
use busylib::model::assets::{AnimationElement, DisplayElement, DisplayElements, Screen};
use busylib::types::app_name::AppName;
use busylib::types::priority::Priority;
use busylib::{ApiPrefix, ClientBuilder, ReqwestHttpTransport};
use clap::{Args, Parser, Subcommand, ValueEnum};
use http::header::HeaderValue;

use crate::error::{CliError, Result};
use crate::transport::LocalToken;

/// Name the animation is stored under in the app's assets. One fixed name, so
/// repeated shows overwrite instead of filling the bar's storage.
const ASSET_NAME: &str = "busybody.anim";
const ELEMENT_ID: &str = "busybody";

/// Put an animated GIF, WebP, or APNG on a BUSY Bar
#[derive(Debug, Parser)]
#[command(name = "busybody", version)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Convert an animation into a .anim file
    Convert {
        /// GIF, animated WebP, or APNG to convert
        #[arg(value_name = "IMAGE")]
        image: PathBuf,

        /// Where to write the .anim file (default: the input's name with .anim)
        #[arg(long, short, value_name = "FILE")]
        output: Option<PathBuf>,

        /// Screen to size and colour the animation for
        #[arg(long, value_enum, default_value_t = ScreenArg::Front)]
        screen: ScreenArg,
    },

    /// Convert an animation, upload it, and play it on the bar
    Show {
        /// GIF, animated WebP, or APNG to show
        #[arg(value_name = "IMAGE")]
        image: PathBuf,

        /// Screen to show it on
        #[arg(long, value_enum, default_value_t = ScreenArg::Front)]
        screen: ScreenArg,

        /// Seconds to show it for; 0 keeps it up until cleared
        #[arg(long, short, default_value_t = 10, value_name = "SECONDS")]
        seconds: u32,

        /// Play once instead of looping
        #[arg(long)]
        once: bool,

        /// Draw priority; an active BUSY session sits at 90
        #[arg(long, default_value = "50", value_name = "1-100")]
        priority: Priority,

        /// Application name the asset and element are filed under
        #[arg(long, short, default_value = "busybody", value_name = "NAME")]
        app: AppName,

        #[command(flatten)]
        device: Device,
    },
}

/// Same flags and env vars as the `busybar` CLI, so the two compose. They
/// hang off `show` rather than the root because `convert` never talks to a bar.
#[derive(Debug, Args)]
struct Device {
    /// Base URL of the device
    #[arg(
        long,
        env = "BUSYBAR_URL",
        value_name = "URL",
        default_value = "http://10.0.4.20"
    )]
    url: String,

    /// BUSY Cloud API token (Authorization: Bearer)
    #[arg(
        long,
        env = "BUSYBAR_TOKEN",
        value_name = "TOKEN",
        hide_env_values = true
    )]
    token: Option<String>,

    /// Local API password from the bar's web interface, for Wi-Fi access (x-api-token)
    #[arg(
        long,
        env = "BUSYBAR_API_TOKEN",
        value_name = "TOKEN",
        hide_env_values = true
    )]
    api_token: Option<String>,

    /// Path the API is mounted under: `/api` on a bar, `/busybar` on BUSY Cloud
    #[arg(long, value_enum, default_value_t = ApiPrefixArg::Device)]
    api_prefix: ApiPrefixArg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ScreenArg {
    /// 72x16, true colour
    Front,
    /// 160x80, 16-level grey
    Back,
}

impl ScreenArg {
    fn target(self) -> Target {
        match self {
            ScreenArg::Front => Target::FRONT,
            ScreenArg::Back => Target::BACK,
        }
    }
}

impl From<ScreenArg> for Screen {
    fn from(arg: ScreenArg) -> Self {
        match arg {
            ScreenArg::Front => Screen::Front,
            ScreenArg::Back => Screen::Back,
        }
    }
}

impl fmt::Display for ScreenArg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ScreenArg::Front => "front",
            ScreenArg::Back => "back",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ApiPrefixArg {
    Device,
    Cloud,
}

impl From<ApiPrefixArg> for ApiPrefix {
    fn from(arg: ApiPrefixArg) -> Self {
        match arg {
            ApiPrefixArg::Device => ApiPrefix::Device,
            ApiPrefixArg::Cloud => ApiPrefix::Cloud,
        }
    }
}

impl Cli {
    pub async fn run(self) -> Result<()> {
        match self.command {
            Command::Convert {
                image,
                output,
                screen,
            } => convert(&image, output, screen),
            Command::Show {
                image,
                screen,
                seconds,
                once,
                priority,
                app,
                device,
            } => show(&image, screen, seconds, once, priority, app, device).await,
        }
    }
}

/// Read `image` and convert it for `screen`.
fn convert_file(image: &Path, screen: ScreenArg) -> Result<Vec<u8>> {
    let data = std::fs::read(image).map_err(|source| CliError::Read {
        path: image.to_path_buf(),
        source,
    })?;
    busybar_anim::convert(&data, screen.target()).map_err(|source| CliError::Convert {
        path: image.to_path_buf(),
        source,
    })
}

fn convert(image: &Path, output: Option<PathBuf>, screen: ScreenArg) -> Result<()> {
    let output = output.unwrap_or_else(|| image.with_extension("anim"));
    let anim = convert_file(image, screen)?;
    std::fs::write(&output, anim).map_err(|source| CliError::Write {
        path: output.clone(),
        source,
    })?;
    println!("wrote {}", output.display());
    Ok(())
}

async fn show(
    image: &Path,
    screen: ScreenArg,
    seconds: u32,
    once: bool,
    priority: Priority,
    app: AppName,
    device: Device,
) -> Result<()> {
    let anim = convert_file(image, screen)?;

    let mut builder = ClientBuilder::new(&device.url)?.api_prefix(device.api_prefix.into());
    if let Some(token) = device.token {
        builder = builder.token(token)?;
    }
    let client = builder.build(LocalToken {
        inner: ReqwestHttpTransport::new(),
        token: device
            .api_token
            .as_deref()
            .map(HeaderValue::from_str)
            .transpose()?,
    });

    client.assets().upload(&app, ASSET_NAME, anim).await?;

    let element = DisplayElement::builder(ELEMENT_ID)?
        .at(0, 0)
        .screen(screen.into())
        .timeout_secs(seconds)
        .animation(AnimationElement::asset(ASSET_NAME)?.repeat(!once));
    let elements = DisplayElements::new(&app)?
        .priority(priority)
        .element(element);
    client.assets().draw(&elements).await?;

    let until = if seconds == 0 {
        "until cleared".to_string()
    } else {
        format!("for {seconds}s")
    };
    println!("showing {} on the {screen} screen {until}", image.display());
    Ok(())
}
