use anyhow::Result;
use clap::{Parser, Subcommand};
use med_ai::{preflight_ai_request, AiDraftRequest};

#[derive(Debug, Parser)]
#[command(name = "med")]
#[command(about = "Local-first medical documentation CLI and TUI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize the local medical data directory.
    Init,

    /// Launch the Ratatui dashboard.
    Tui,

    /// Patient commands.
    Patient {
        #[command(subcommand)]
        command: PatientCommand,
    },

    /// Compliance and BAA registry commands.
    Vendor {
        #[command(subcommand)]
        command: VendorCommand,
    },

    /// AI preflight checks.
    Ai {
        #[command(subcommand)]
        command: AiCommand,
    },
}

#[derive(Debug, Subcommand)]
enum PatientCommand {
    /// List patients.
    List,

    /// Add a synthetic placeholder patient.
    Add { display_name: String },
}

#[derive(Debug, Subcommand)]
enum VendorCommand {
    /// List configured vendors.
    List,

    /// Explain the BAA gate.
    Explain,
}

#[derive(Debug, Subcommand)]
enum AiCommand {
    /// Run a local preflight example that must block PHI without a BAA.
    PreflightDemo,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Init => {
            println!("Initialize local encrypted storage at ~/.medical-cli (planned).");
            println!("Do not store PHI in this repository.");
        }
        Command::Tui => med_tui::run()?,
        Command::Patient { command } => match command {
            PatientCommand::List => {
                println!("Patient list is not connected to storage yet.");
                println!("Next step: implement med-store repository methods.");
            }
            PatientCommand::Add { display_name } => {
                println!("Would add patient: {display_name}");
                println!("Storage implementation is planned.");
            }
        },
        Command::Vendor { command } => match command {
            VendorCommand::List => {
                println!("No vendors are approved for PHI by default.");
            }
            VendorCommand::Explain => {
                println!("AI API PHI calls require an executed BAA and local approval.");
                println!("No BAA record means the request is blocked.");
            }
        },
        Command::Ai { command } => match command {
            AiCommand::PreflightDemo => {
                let request = AiDraftRequest {
                    contains_phi: true,
                    service_name: "example-model".to_owned(),
                    instruction: "draft a note".to_owned(),
                    clinical_context: "synthetic demo context".to_owned(),
                };

                let today = time::Date::from_calendar_date(2026, time::Month::May, 28).unwrap();
                match preflight_ai_request(None, &request, today) {
                    Ok(()) => println!("AI request allowed."),
                    Err(error) => println!("AI request blocked: {error}"),
                }
            }
        },
    }

    Ok(())
}
