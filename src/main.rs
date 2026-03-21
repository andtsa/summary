//! print a codebase to a text file
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::ensure;
use clap::{Parser, Subcommand};
use clio::Output;
use glob::glob;
use serde_derive::Deserialize;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[clap(name = "summary")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Summarise a codebase to a text file
    Run {
        /// The configuration file used for this summary
        #[arg(short, long)]
        source: PathBuf,

        /// The output file (defaults to stdout)
        #[clap(long, short, value_parser, default_value = "-")]
        output: Output,
    },
    /// Create a template configuration file
    Init {
        /// Where to write the config file (defaults to ./summary.toml)
        #[arg(short, long, default_value = "summary.toml")]
        output: PathBuf,

        /// Overwrite the file if it already exists
        #[arg(long, default_value_t = false)]
        force: bool,
    },
}

#[derive(Debug, Deserialize)]
struct Config {
    /// the project name to be placed at the start of the output file
    name: String,
    /// the project root directory
    root: PathBuf,
    /// a list of files to include
    #[serde(default)]
    files: Vec<PathBuf>,
    /// a list of directories from which to include all files
    #[serde(default)]
    directories: Vec<PathBuf>,
    /// globs for what to include
    #[serde(default)]
    globs: Vec<String>,
    /// which file extensions to consider
    /// any file with an extension that's not here will be ignored
    /// (e.g. .DS_Store)
    #[serde(default)]
    extensions: Vec<String>,
    /// exclude these globs even if they're matched by something else
    #[serde(default)]
    exclude: Vec<String>,
}

const TEMPLATE: &str = r#"# summary.toml — codebase summary configuration

# Project name, printed at the top of the output file.
name = "my-project"

# The root directory of the project. All relative paths below are resolved
# from here. Usually the directory that contains this config file.
root = "."

# Individual files to always include, relative to `root`.
files = [
    "README.md",
    "Cargo.toml",
]

# Directories to include recursively, relative to `root`.
# Every file inside (at any depth) is added to the search space.
directories = [
    "src",
]

# Glob patterns, relative to `root`.
# Useful for targeting specific file types across many directories.
globs = [
    "docs/**/*.md",
    "scripts/**/*.sh",
]

# Extension allowlist. Only files whose extension matches an entry here
# will be written to the output. The leading dot is optional.
# Leave the list empty ([]) to allow every extension.
extensions = [
    "rs",
    "toml",
    "md",
    "sh",
    "py",
    "ts",
    "js",
]

# Patterns to always exclude, even if matched by files/directories/globs above.
exclude = [
    "target/**",
    "**/*.lock",
    "**/*.snap",
]

"#;

fn cmd_init(output: PathBuf, force: bool) -> anyhow::Result<()> {
    ensure!(
        force || !output.exists(),
        "{} already exists — pass --force to overwrite",
        output.display()
    );
    std::fs::write(&output, TEMPLATE)?;
    println!("wrote template config to {}", output.display());
    Ok(())
}

fn cmd_run(source: PathBuf, mut output: Output) -> anyhow::Result<()> {
    let start = Instant::now();
    ensure!(
        source.is_file(),
        "{} is not a config file",
        source.display()
    );
    println!("summarising project from {}", source.display());

    // parse config
    let text = std::fs::read_to_string(&source)?;
    let cfg: Config = toml::from_str(text.as_str())?;

    ensure!(
        cfg.root.is_dir(),
        "{} is not a directory",
        cfg.root.display()
    );

    let o = &mut output;

    writeln!(o, "# project {} codebase summary:\n", cfg.name)?;

    // run `tree --gitignore` and write its output
    match std::process::Command::new("tree")
        .arg("--gitignore")
        .current_dir(&cfg.root)
        .output()
    {
        Ok(tree_output) if tree_output.status.success() => {
            writeln!(o, "## directory tree\n")?;
            writeln!(o, "```")?;
            o.write_all(&tree_output.stdout)?;
            writeln!(o, "```\n")?;
        }
        _ => {
            eprintln!("warning: `tree` not available or failed, skipping directory tree");
        }
    }

    // build the search space, deduplicating via a seen set
    let mut search_space: Vec<PathBuf> = Vec::new();
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    let mut add = |path: PathBuf| {
        if let Ok(canonical) = path.canonicalize()
            && seen.insert(canonical)
        {
            search_space.push(path);
        }
    };

    // explicit files
    for f in &cfg.files {
        let path = cfg.root.join(f);
        ensure!(path.is_file(), "file {} does not exist", path.display());
        add(path);
    }

    // directories — walk recursively
    for dir in &cfg.directories {
        let path = cfg.root.join(dir);
        ensure!(path.is_dir(), "directory {} does not exist", path.display());
        for entry in walkdir::WalkDir::new(&path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            add(entry.into_path());
        }
    }

    // globs — resolved relative to root
    for pattern in &cfg.globs {
        let full_pattern = cfg.root.join(pattern);
        let pattern_str = full_pattern.to_string_lossy();
        for entry in glob(&pattern_str)?.filter_map(|e| e.ok()) {
            if entry.is_file() {
                add(entry);
            }
        }
    }

    // filter by extension
    let extensions: std::collections::HashSet<String> = cfg
        .extensions
        .iter()
        .map(|e| e.trim_start_matches('.').to_lowercase())
        .collect();

    let search_space: Vec<PathBuf> = if extensions.is_empty() {
        search_space
    } else {
        search_space
            .into_iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| extensions.contains(&e.to_lowercase()))
                    .unwrap_or(false)
            })
            .collect()
    };

    let exclude_patterns: Vec<glob::Pattern> = cfg
        .exclude
        .iter()
        .filter_map(|g| glob::Pattern::new(g).ok())
        .collect();

    let search_space: Vec<PathBuf> = search_space
        .into_iter()
        .filter(|p| {
            let rel = p.strip_prefix(&cfg.root).unwrap_or(p);
            !exclude_patterns.iter().any(|pat| pat.matches_path(rel))
        })
        .collect();

    // write each file
    writeln!(o, "## files\n")?;
    let mut n_files = 0;

    for path in &search_space {
        let rel = path.strip_prefix(&cfg.root).unwrap_or(path);

        let lang = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        match std::fs::read_to_string(path) {
            Ok(contents) => {
                writeln!(o, "### {}\n", rel.display())?;
                writeln!(o, "```{lang}")?;
                o.write_all(contents.as_bytes())?;
                if !contents.ends_with('\n') {
                    writeln!(o)?;
                }
                writeln!(o, "```\n")?;
                n_files += 1;
            }
            Err(e) => {
                eprintln!("warning: skipping {} — {e}", path.display());
            }
        }
    }

    output.flush()?;
    output.finish()?;

    println!(
        "[done] captured {n_files} files in {}ms",
        start.elapsed().as_millis()
    );

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    match args.command {
        Command::Run { source, output } => cmd_run(source, output),
        Command::Init { output, force } => cmd_init(output, force),
    }
}
