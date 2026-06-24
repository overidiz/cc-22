use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn main() -> nih_plug_xtask::Result<()> {
    prepare_standalone_alias_for_bundler()?;
    nih_plug_xtask::main()
}

fn prepare_standalone_alias_for_bundler() -> nih_plug_xtask::Result<()> {
    let args = env::args().collect::<Vec<_>>();
    let command = args.get(1).map(String::as_str);
    if !matches!(command, Some("bundle" | "bundle-universal")) {
        return Ok(());
    }

    let release = args.iter().any(|arg| arg == "--release");
    let profile = profile_name(&args, release);

    if command == Some("bundle-universal") {
        for target in ["x86_64-apple-darwin", "aarch64-apple-darwin"] {
            build_and_alias_standalone(Some(target), &profile, release)?;
        }
    } else {
        let target = argument_value(&args, "--target");
        build_and_alias_standalone(target.as_deref(), &profile, release)?;
    }

    Ok(())
}

fn build_and_alias_standalone(
    target: Option<&str>,
    profile: &str,
    release: bool,
) -> nih_plug_xtask::Result<()> {
    let mut command = Command::new("cargo");
    command.args(["build", "--bin", "cc22-standalone"]);
    if release {
        command.arg("--release");
    } else if profile != "debug" {
        command.args(["--profile", profile]);
    }
    if let Some(target) = target {
        command.args(["--target", target]);
    }
    command.stdout(Stdio::inherit()).stderr(Stdio::inherit());

    let status = command.status()?;
    if !status.success() {
        return Err(io::Error::other("failed to build the CC-22 standalone binary").into());
    }

    let target_dir = target.map_or_else(
        || PathBuf::from("target"),
        |triple| Path::new("target").join(triple),
    );
    let output_dir = target_dir.join(profile);
    let suffix = executable_suffix(target);
    let source = output_dir.join(format!("cc22-standalone{suffix}"));
    let destination = output_dir.join(format!("cc_22{suffix}"));
    fs::copy(&source, &destination).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "failed to prepare standalone alias '{}' from '{}': {error}",
                destination.display(),
                source.display()
            ),
        )
    })?;

    Ok(())
}

fn profile_name(args: &[String], release: bool) -> String {
    if release {
        return "release".to_owned();
    }
    argument_value(args, "--profile").unwrap_or_else(|| "debug".to_owned())
}

fn argument_value(args: &[String], option: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == option)
        .map(|pair| pair[1].clone())
        .or_else(|| {
            let prefix = format!("{option}=");
            args.iter()
                .find_map(|arg| arg.strip_prefix(&prefix).map(str::to_owned))
        })
}

fn executable_suffix(target: Option<&str>) -> &'static str {
    match target {
        Some(target) if target.contains("windows") => ".exe",
        Some(_) => "",
        None => env::consts::EXE_SUFFIX,
    }
}
