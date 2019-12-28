use cargo::core::Workspace;
use cargo::util::important_paths::find_root_manifest_for_wd;
use cargo::util::Config;
use cargo_flutter::EngineInfo;
use clap::{App, AppSettings, Arg, SubCommand};
use failure::format_err;
use serde::Deserialize;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::{exit, Command, ExitStatus};
use std::{env, fs, str};

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let app_matches = App::new("cargo-flutter")
        .bin_name("cargo")
        .subcommand(
            SubCommand::with_name("flutter")
                .setting(AppSettings::TrailingVarArg)
                .version(env!("CARGO_PKG_VERSION"))
                .author("flutter-rs")
                .about("Provides a smooth experience for developing flutter-rs apps.")
                .arg(
                    Arg::with_name("cargo-args")
                        .value_name("CARGO_ARGS")
                        .takes_value(true)
                        .required(true)
                        .multiple(true),
                ),
        )
        .get_matches();

    let matches = if let Some(matches) = app_matches.subcommand_matches("flutter") {
        matches
    } else {
        eprintln!("This binary may only be called via `cargo flutter`.");
        exit(1);
    };

    let cargo_args: Vec<&str> = matches
        .values_of("cargo-args")
        .expect("cargo-args to not be null")
        .collect();

    let target = get_arg(&cargo_args, |f| f == "--target");
    let package = get_arg(&cargo_args, |f| f == "--package" || f == "-p");

    let cargo_config = Config::default()?;
    let root_manifest = find_root_manifest_for_wd(cargo_config.cwd())?;
    let workspace = Workspace::new(&root_manifest, &cargo_config)?;

    let config = load_config(&workspace, &package)?;

    let version = config
        .version
        .unwrap_or(cargo_flutter::get_flutter_version()?);
    let rustc = cargo_config.load_global_rustc(Some(&workspace))?;
    let target = target.unwrap_or(rustc.host.as_str()).to_string();

    let engine_path = download(version, target)?;

    let status = run(&cargo_config, &cargo_args, &engine_path);

    exit(status.code().unwrap_or(-1));
}

fn get_arg<'a, F: Fn(&str) -> bool>(args: &'a [&str], matches: F) -> Option<&'a str> {
    args.into_iter()
        .position(|f| matches(*f))
        .map(|pos| args.into_iter().nth(pos + 1))
        .unwrap_or_default()
        .map(|r| *r)
}

fn load_config(
    workspace: &Workspace,
    package: &Option<&str>,
) -> Result<TomlFlutter, Box<dyn Error>> {
    let package = if let Some(package) = package {
        workspace
            .members()
            .find(|pkg| &pkg.name().as_str() == package)
            .ok_or_else(|| format_err!("package `{}` is not a member of the workspace", package))?
    } else {
        workspace.current()?
    };

    let bytes = fs::read(package.manifest_path())?;
    let string = str::from_utf8(&bytes)?;
    let config: TomlConfig = toml::from_str(string)?;
    let flutter = config
        .package
        .metadata
        .unwrap_or_default()
        .flutter
        .unwrap_or_default();
    Ok(flutter)
}

fn download(version: String, target: String) -> Result<PathBuf, Box<dyn Error>> {
    log::debug!("Engine version is {:?}", version);

    let info = EngineInfo::new(version, target);
    println!("Checking flutter engine status");

    if let Ok(tx) = info.download() {
        for (total, done) in tx.iter() {
            println!("Downloading flutter engine {} of {}", done, total);
        }
    }

    let engine_path = info.engine_path();
    log::debug!("Engine path is {:?}", engine_path);
    Ok(engine_path)
}

fn run(cargo_config: &Config, cargo_args: &[&str], engine_path: &Path) -> ExitStatus {
    Command::new("cargo")
        .current_dir(cargo_config.cwd())
        .env(
            "RUSTFLAGS",
            format!("-Clink-arg=-L{}", engine_path.parent().unwrap().display()),
        )
        .args(cargo_args)
        .status()
        .expect("Success")
}

#[derive(Debug, Clone, Deserialize)]
struct TomlConfig {
    package: TomlPackage,
}

#[derive(Debug, Clone, Deserialize)]
struct TomlPackage {
    name: String,
    metadata: Option<TomlMetadata>,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct TomlMetadata {
    flutter: Option<TomlFlutter>,
}

#[derive(Debug, Default, Clone, Deserialize)]
struct TomlFlutter {
    version: Option<String>,
}
