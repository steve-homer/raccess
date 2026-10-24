mod extract;
mod model;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use jetdb::PageReader;

use model::Status;

/// Extract data and schema from Microsoft Access databases.
#[derive(Parser)]
#[command(name = "raccess", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Report the detected format of an Access database.
    Info {
        /// Path to an .mdb or .accdb file.
        database: PathBuf,
        #[arg(long)]
        password: Option<String>,
    },

    /// Extract every object into an output directory.
    Extract {
        /// Path to an .mdb or .accdb file.
        database: PathBuf,

        /// Directory to write into; created if it does not exist.
        #[arg(short, long)]
        output: PathBuf,

        #[arg(long)]
        password: Option<String>,

        /// Also export MSys* tables and hidden `~sq_` queries.
        #[arg(long)]
        include_system: bool,

        /// Text written for a NULL value.
        #[arg(long, default_value = "", value_name = "TEXT")]
        null_as: String,
    },
}

fn open(database: &PathBuf, password: Option<&str>) -> Result<PageReader> {
    PageReader::open_with_password(database, password)
        .with_context(|| format!("opening {}", database.display()))
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    match run() {
        Ok(status) => match status {
            Status::Ok => ExitCode::SUCCESS,
            // Partial and failed objects are in the manifest; the exit code
            // lets a script gate on them without parsing it.
            Status::Partial => ExitCode::from(1),
            Status::Failed => ExitCode::from(2),
        },
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(3)
        }
    }
}

fn run() -> Result<Status> {
    match Cli::parse().command {
        Command::Info { database, password } => {
            let reader = open(&database, password.as_deref())?;
            let header = reader.header();
            println!("file:      {}", database.display());
            println!("format:    {}", header.version);
            println!("page size: {}", header.format.page_size);
            println!("code page: {}", header.code_page);
            println!("language:  {}", header.lang_id);
            // The Jet page-encoding key, which jetdb applies transparently.
            // Office-encrypted .accdb files fail to open without --password
            // instead, so this is not a general "is it encrypted" flag.
            println!("jet key:   {:#010x}", header.db_key);
            Ok(Status::Ok)
        }

        Command::Extract {
            database,
            output,
            password,
            include_system,
            null_as,
        } => {
            let mut reader = open(&database, password.as_deref())?;
            let options = extract::Options {
                include_system,
                null_as,
            };
            let manifest = extract::run(&mut reader, &database, &output, &options)?;

            let total = manifest.objects.len();
            let failed = manifest
                .objects
                .iter()
                .filter(|o| o.status == Status::Failed)
                .count();
            println!(
                "extracted {} object{} to {}{}",
                total,
                if total == 1 { "" } else { "s" },
                output.display(),
                if failed == 0 {
                    String::new()
                } else {
                    format!(" ({failed} failed; see manifest.json)")
                }
            );
            Ok(extract::worst_status(&manifest.objects))
        }
    }
}
