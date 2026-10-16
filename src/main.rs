mod gpx;
mod raw_json;
mod render;
mod summary;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use jiff::tz::TimeZone;

const CAT: &str = r"
     /\_/\                  /\
    ( o.o )         /\    _/  \       /\
     > ^ <         /  \__/     \     /  \
    /     \    ___/             \___/    \
   (_)-(_) \__/                           \___
 gpxcat  GPX を cat して集計する";

#[derive(Parser)]
#[command(name = "gpxcat", version)]
#[command(about = "GPX ファイルを読み、距離・標高・時間を集計して出力する")]
#[command(before_help = CAT)]
struct Cli {
    #[arg(help = "読み込む GPX ファイル")]
    file: PathBuf,

    #[arg(long, value_enum, default_value_t = Format::Text, help = "出力形式")]
    format: Format,

    #[arg(
        long,
        value_name = "IANA名",
        default_value = "Asia/Tokyo",
        value_parser = parse_tz,
        help = "表示に使うタイムゾーン"
    )]
    tz: TimeZone,

    #[arg(long, help = "1 km ごとの区間のペースと速度も出力する")]
    detail: bool,

    #[arg(
        long,
        conflicts_with_all = ["format", "tz", "detail"],
        help = "集計せず、GPX の XML 全体をそのまま JSON に変換して出力する"
    )]
    raw_json: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
}

fn parse_tz(name: &str) -> Result<TimeZone, String> {
    TimeZone::get(name).map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("gpxcat: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> Result<String, String> {
    let xml = std::fs::read_to_string(&cli.file)
        .map_err(|e| format!("{} を読み込めません: {e}", cli.file.display()))?;
    if cli.raw_json {
        return raw_json::convert(&xml).map_err(|e| format!("{}: {e}", cli.file.display()));
    }
    let file_name = cli
        .file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let track = gpx::parse(&xml, &file_name).map_err(|e| format!("{}: {e}", cli.file.display()))?;
    let summary =
        summary::summarize(&track, &cli.tz).map_err(|e| format!("{}: {e}", cli.file.display()))?;
    Ok(match cli.format {
        Format::Text => render::text::render(&summary, cli.detail),
        Format::Json => render::json::render(&summary, cli.detail),
    })
}
