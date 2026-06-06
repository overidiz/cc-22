use std::{
    env, fs, io,
    path::PathBuf,
    process::{Command, Stdio},
};

fn main() -> nih_plug_xtask::Result<()> {
    prepare_standalone_alias_for_bundler()?;
    nih_plug_xtask::main()
}

fn prepare_standalone_alias_for_bundler() -> nih_plug_xtask::Result<()> {
    let args = env::args().collect::<Vec<_>>();
    if !args.iter().any(|arg| arg == "bundle") {
        return Ok(());
    }

    let release = args.iter().any(|arg| arg == "--release");
    let mut command = Command::new("cargo");
    command.args(["build", "--bin", "cc22-standalone"]);
    if release {
        command.arg("--release");
    }
    command.stdout(Stdio::inherit()).stderr(Stdio::inherit());

    let status = command.status()?;
    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "failed to build the CC-22 standalone binary",
        )
        .into());
    }

    let profile = if release { "release" } else { "debug" };
    let target_dir = PathBuf::from("target").join(profile);
    let source = target_dir.join(format!("cc22-standalone{}", env::consts::EXE_SUFFIX));
    let destination = target_dir.join(format!("cc_22{}", env::consts::EXE_SUFFIX));

    fs::copy(&source, &destination)?;
    Ok(())
}
