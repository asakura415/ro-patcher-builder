use std::path::PathBuf;
use clap::Parser;
use patch_maker::{add_patch_to_plist, build_thor_from_directory, ThorBuildOptions};

#[derive(Parser, Debug)]
#[command(name = "patch-maker", about = "Modern RO Patch Archive (.thor) Creator", version)]
struct Cli {
    /// Directory containing the files to package (e.g. data/, System/)
    #[arg(short, long)]
    input_dir: PathBuf,

    /// Output .thor file path
    #[arg(short, long)]
    output_file: PathBuf,

    /// Merge into target GRF (defaults to true; set to false for root game directory updates)
    #[arg(long, default_value_t = true)]
    grf_merge: bool,

    /// Target GRF name when grf_merge is true (e.g. data.grf or _prontera_box.grf)
    #[arg(long, default_value = "data.grf")]
    target_grf: String,

    /// Optional plist.txt path to automatically append this patch to
    #[arg(long)]
    update_plist: Option<PathBuf>,

    /// Specific patch index for plist.txt (defaults to auto-increment)
    #[arg(long)]
    patch_index: Option<u32>,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    println!("Creating THOR patch from: {:?}", cli.input_dir);
    println!("Output path: {:?}", cli.output_file);

    let options = ThorBuildOptions {
        use_grf_merging: cli.grf_merge,
        target_grf_name: if cli.grf_merge { Some(cli.target_grf) } else { None },
        include_checksums: true,
        ..Default::default()
    };

    let count = build_thor_from_directory(&cli.input_dir, &cli.output_file, &options)?;
    println!("Successfully packaged {} files into {:?}", count, cli.output_file);

    if let Some(plist_path) = cli.update_plist {
        let thor_name = cli
            .output_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("patch.thor");

        let index = add_patch_to_plist(&plist_path, thor_name, cli.patch_index)?;
        println!("Updated {:?} with patch index {} -> {}", plist_path, index, thor_name);
    }

    Ok(())
}
