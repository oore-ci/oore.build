//! `oore notes show|set|draft`: read and write the release notes of an app
//! version. Every build of the version, its tester page and its store
//! uploads and promotions use the same notes.

use std::path::PathBuf;

use anyhow::Context;
use clap::{Args, Subcommand};
use oore_contract::{
    ReleaseNotesDraftResponse, ReleaseNotesSource, UpdateVersionReleaseNotesRequest,
    VersionReleaseNotes,
};

use crate::build_commands::{Api, ApiConnectionArgs};

#[derive(Debug, Args)]
pub(crate) struct NotesArgs {
    #[command(subcommand)]
    pub(crate) command: NotesSubcommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum NotesSubcommand {
    /// Print the release notes of an app version.
    Show(NotesShowArgs),
    /// Write the release notes of an app version.
    Set(NotesSetArgs),
    /// Print notes drafted from the commits since the last released version.
    /// Nothing is saved.
    Draft(NotesShowArgs),
}

/// The app version whose notes to read or write.
#[derive(Debug, Args)]
pub(crate) struct VersionArgs {
    /// Project the app belongs to.
    #[arg(long)]
    project: String,

    /// App version, as in pubspec.yaml without the +build number, such as 2.5.1.
    #[arg(long = "version", value_name = "VERSION")]
    version_name: String,
}

#[derive(Debug, Args)]
pub(crate) struct NotesShowArgs {
    #[command(flatten)]
    version: VersionArgs,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the daemon's response as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct NotesSetArgs {
    #[command(flatten)]
    version: VersionArgs,

    /// The notes, in the default language (en-US unless the pipeline says
    /// otherwise).
    #[arg(long, value_name = "TEXT", conflicts_with = "file")]
    text: Option<String>,

    /// Read the notes from a file, or from standard input with `-`.
    #[arg(long, value_name = "PATH")]
    file: Option<PathBuf>,

    /// Notes in another language, as LOCALE=PATH, such as de-DE=notes/de.txt.
    /// Repeat for several. Replaces the languages saved before.
    #[arg(long = "locale", value_name = "LOCALE=PATH")]
    locales: Vec<String>,

    #[command(flatten)]
    connection: ApiConnectionArgs,

    /// Print the saved notes as JSON.
    #[arg(long, default_value = "false")]
    json: bool,
}

fn notes_url(api: &Api, version: &VersionArgs) -> anyhow::Result<url::Url> {
    let mut url = url::Url::parse(&api.daemon_url).context("invalid daemon URL")?;
    url.path_segments_mut()
        .map_err(|()| anyhow::anyhow!("invalid daemon URL"))?
        .pop_if_empty()
        .extend([
            "v1",
            "projects",
            version.project.trim(),
            "release-notes",
            version.version_name.trim(),
        ]);
    Ok(url)
}

fn source_label(notes: &VersionReleaseNotes) -> String {
    match notes.source {
        ReleaseNotesSource::Repository => format!(
            "From {} in the repository",
            notes.source_path.as_deref().unwrap_or("a file")
        ),
        ReleaseNotesSource::Manual => match &notes.updated_by {
            Some(by) => format!("Written by {by}"),
            None => "Written in Oore".to_string(),
        },
        ReleaseNotesSource::Commits => match &notes.drafted_since {
            Some(since) => format!("Drafted from commits since {since}"),
            None => "Drafted from commits".to_string(),
        },
    }
}

/// What `oore notes show` prints.
fn render_notes(notes: &VersionReleaseNotes) -> String {
    let mut out = format!(
        "Release notes for {} ({})\n\n{}\n",
        notes.version_name,
        source_label(notes),
        notes.text.trim_end()
    );
    for locale in &notes.locales {
        out.push_str(&format!(
            "\n[{}]\n{}\n",
            locale.locale,
            locale.text.trim_end()
        ));
    }
    out
}

fn read_text(path: &PathBuf) -> anyhow::Result<String> {
    if path.as_os_str() == "-" {
        return std::io::read_to_string(std::io::stdin()).context("failed to read standard input");
    }
    std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

fn update_request(args: &NotesSetArgs) -> anyhow::Result<UpdateVersionReleaseNotesRequest> {
    let text = match (&args.text, &args.file) {
        (Some(text), _) => text.clone(),
        (None, Some(path)) => read_text(path)?,
        (None, None) => anyhow::bail!("pass the notes with --text or --file"),
    };
    let locales = args
        .locales
        .iter()
        .map(|entry| {
            let (locale, path) = entry
                .split_once('=')
                .with_context(|| format!("--locale {entry}: use LOCALE=PATH"))?;
            Ok(oore_contract::ReleaseNotesLocale {
                locale: locale.trim().to_string(),
                text: read_text(&PathBuf::from(path.trim()))?.trim().to_string(),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(UpdateVersionReleaseNotesRequest {
        text: text.trim().to_string(),
        locales,
    })
}

pub(crate) async fn handle_notes(args: NotesArgs) -> anyhow::Result<()> {
    match args.command {
        NotesSubcommand::Show(args) => {
            let api = Api::connect(&args.connection)?;
            let notes: VersionReleaseNotes = api
                .send(
                    api.client.get(notes_url(&api, &args.version)?),
                    "release notes lookup",
                )
                .await?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&notes)?);
            } else {
                print!("{}", render_notes(&notes));
            }
        }
        NotesSubcommand::Set(args) => {
            let request = update_request(&args)?;
            let api = Api::connect(&args.connection)?;
            let notes: VersionReleaseNotes = api
                .send(
                    api.client
                        .put(notes_url(&api, &args.version)?)
                        .json(&request),
                    "release notes update",
                )
                .await?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&notes)?);
            } else {
                println!(
                    "Saved the release notes for {}. Every build of it, and its next store upload or promotion, uses them.",
                    notes.version_name
                );
            }
        }
        NotesSubcommand::Draft(args) => {
            let api = Api::connect(&args.connection)?;
            let draft: ReleaseNotesDraftResponse = api
                .send(
                    api.client.post({
                        let mut url = notes_url(&api, &args.version)?;
                        url.path_segments_mut()
                            .map_err(|()| anyhow::anyhow!("invalid daemon URL"))?
                            .push("draft");
                        url
                    }),
                    "release notes draft",
                )
                .await?;
            if args.json {
                println!("{}", serde_json::to_string_pretty(&draft)?);
            } else if draft.text.trim().is_empty() {
                println!("No commits to draft from yet.");
            } else {
                println!("{}", draft.text.trim_end());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(source: ReleaseNotesSource) -> VersionReleaseNotes {
        VersionReleaseNotes {
            project_id: "p".into(),
            version_name: "2.5.1".into(),
            text: "- Faster sync\n".into(),
            locales: vec![oore_contract::ReleaseNotesLocale {
                locale: "de-DE".into(),
                text: "- Schneller".into(),
            }],
            source,
            source_path: Some("CHANGELOG.md".into()),
            drafted_since: None,
            updated_at: 0,
            updated_by: None,
        }
    }

    #[test]
    fn show_names_the_source_and_every_language() {
        let out = render_notes(&notes(ReleaseNotesSource::Repository));
        assert_eq!(
            out,
            "Release notes for 2.5.1 (From CHANGELOG.md in the repository)\n\n- Faster sync\n\n[de-DE]\n- Schneller\n"
        );
    }

    #[test]
    fn set_needs_text_and_reads_locales_as_locale_equals_path() {
        let dir = tempfile::tempdir().unwrap();
        let german = dir.path().join("de.txt");
        std::fs::write(&german, "  Schneller\n").unwrap();
        let args = NotesSetArgs {
            version: VersionArgs {
                project: "p".into(),
                version_name: "2.5.1".into(),
            },
            text: Some(" Faster sync \n".into()),
            file: None,
            locales: vec![format!("de-DE={}", german.display())],
            connection: ApiConnectionArgs::default(),
            json: false,
        };
        let request = update_request(&args).unwrap();
        assert_eq!(request.text, "Faster sync");
        assert_eq!(request.locales[0].locale, "de-DE");
        assert_eq!(request.locales[0].text, "Schneller");

        let missing = NotesSetArgs {
            text: None,
            locales: vec!["de-DE".into()],
            ..args
        };
        assert!(update_request(&missing).is_err());
    }
}
