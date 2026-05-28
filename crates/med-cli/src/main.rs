use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use med_ai::{preflight_ai_request, AiDraftRequest};
use med_core::{new_id, Encounter, EncounterStatus, EncounterType, Patient, PatientId};
use med_store::LocalStore;
use time::OffsetDateTime;

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

    /// Encounter commands.
    Encounter {
        #[command(subcommand)]
        command: EncounterCommand,
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

    /// Add a patient to the local repository.
    Add {
        display_name: String,

        #[arg(long)]
        mrn: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum EncounterCommand {
    /// List encounters for a patient.
    List { patient_id: String },

    /// Create a new in-progress office visit encounter.
    New {
        patient_id: String,

        #[arg(long)]
        reason: Option<String>,
    },
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
            let paths = initialize_local_data_dir()?;
            let _store = open_store()?;
            println!("Initialized local medical data directory:");
            println!("  {}", paths.data_dir.display());
            println!("Database:");
            println!("  {}", paths.database.display());
            println!(
                "Do not store PHI until SQLCipher/key handling has been configured and reviewed."
            );
        }
        Command::Tui => med_tui::run()?,
        Command::Patient { command } => match command {
            PatientCommand::List => {
                let store = open_store()?;
                let patients = store.list_patients()?;

                if patients.is_empty() {
                    println!("No patients found.");
                } else {
                    for patient in patients {
                        let mrn = patient
                            .medical_record_number
                            .unwrap_or_else(|| "-".to_owned());
                        println!("{}\t{}\t{}", patient.id, mrn, patient.display_name);
                    }
                }
            }
            PatientCommand::Add { display_name, mrn } => {
                let store = open_store()?;
                let now = OffsetDateTime::now_utc();
                let patient = Patient {
                    id: new_id(),
                    medical_record_number: mrn,
                    display_name,
                    date_of_birth: None,
                    sex_at_birth: None,
                    created_at: now,
                    updated_at: now,
                };

                store.insert_patient(&patient)?;
                println!("Created patient:");
                println!("  id: {}", patient.id);
                println!("  name: {}", patient.display_name);
            }
        },
        Command::Encounter { command } => match command {
            EncounterCommand::List { patient_id } => {
                let store = open_store()?;
                let patient_id = parse_patient_id(&patient_id)?;
                let encounters = store.list_encounters_for_patient(patient_id)?;

                if encounters.is_empty() {
                    println!("No encounters found for patient {patient_id}.");
                } else {
                    for encounter in encounters {
                        println!(
                            "{}\t{:?}\t{:?}\t{}",
                            encounter.id,
                            encounter.encounter_type,
                            encounter.status,
                            encounter.reason.unwrap_or_else(|| "-".to_owned())
                        );
                    }
                }
            }
            EncounterCommand::New { patient_id, reason } => {
                let store = open_store()?;
                let patient_id = parse_patient_id(&patient_id)?;

                if store.get_patient(patient_id)?.is_none() {
                    anyhow::bail!("patient {patient_id} does not exist");
                }

                let encounter = Encounter {
                    id: new_id(),
                    patient_id,
                    practitioner_id: None,
                    encounter_type: EncounterType::OfficeVisit,
                    status: EncounterStatus::InProgress,
                    started_at: OffsetDateTime::now_utc(),
                    ended_at: None,
                    reason,
                };

                store.insert_encounter(&encounter)?;
                println!("Created encounter:");
                println!("  id: {}", encounter.id);
                println!("  patient_id: {}", encounter.patient_id);
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

struct LocalPaths {
    data_dir: PathBuf,
    database: PathBuf,
}

fn initialize_local_data_dir() -> Result<LocalPaths> {
    let paths = local_paths()?;
    fs::create_dir_all(&paths.data_dir)?;
    fs::create_dir_all(paths.data_dir.join("attachments"))?;
    fs::create_dir_all(paths.data_dir.join("backups"))?;
    fs::create_dir_all(paths.data_dir.join("exports"))?;
    Ok(paths)
}

fn open_store() -> Result<LocalStore> {
    let paths = initialize_local_data_dir()?;
    let key = std::env::var("MEDCLI_DB_KEY")
        .unwrap_or_else(|_| "development-only-default-key-not-for-production-phi".to_owned());

    Ok(LocalStore::open_encrypted(paths.database, &key)?)
}

fn local_paths() -> Result<LocalPaths> {
    let data_dir = match std::env::var_os("MEDCLI_DATA_DIR") {
        Some(value) => PathBuf::from(value),
        None => home_dir()?.join(".medical-cli"),
    };

    let database = data_dir.join("records.db");
    Ok(LocalPaths { data_dir, database })
}

fn home_dir() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("USERPROFILE") {
        return Ok(PathBuf::from(home));
    }

    if let Some(home) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(home));
    }

    anyhow::bail!("could not determine home directory; set MEDCLI_DATA_DIR")
}

fn parse_patient_id(value: &str) -> Result<PatientId> {
    Ok(uuid::Uuid::parse_str(value)?)
}
