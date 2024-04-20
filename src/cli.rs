use clap::{Args, Parser, Subcommand};
use crate::common::ObjectType;

/// a subset of git, implemented as a learning challenge
#[derive(Parser)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Create an empty Git repository
    Init,
    /// Provide content for repository objects
    CatFile {
        #[clap(flatten)]
        flags: CatFlags,
        /// output the unparsed deflated data for tree objects
        #[arg(long)]
        force_raw: bool,
        /// sha1 hash
        object: String,
    },
    /// Compute object ID and optionally create an object from a file
    HashObject {
        /// Specify the type of object to be created
        #[arg(value_enum, default_value = "blob", short = 't')]
        object_type: ObjectType,
        /// Actually write the object into the object database
        #[arg(short)]
        write: bool,
        /// file path
        file: String,
    },
    /// List the contents of a tree object
    LsTree {
        /// List only filenames
        #[arg(long)]
        name_only: bool,
        /// sha1 hash
        tree_sha: String,
    },
    /// Create a tree object from the current index
    WriteTree {
        /// Only print the hash, do not actually write the objects
        #[arg(long)]
        dry_run: bool,
    },
    /// Create a new commit object
    CommitTree {
        // todo: currently handle only 1 parent, can't handle multiple parents (merge commits)
        /// The id of a parent commit object. Can be empty for the initial commit
        #[arg(short)]
        parent: Option<String>,
        // todo: actual git allows multiple messages (and they get concatenated)
        /// A paragraph in the commit log message
        #[arg(short)]
        message: String,
        /// Only print the hash, do not actually write the commit object
        #[arg(long)]
        dry_run: bool,
        /// An existing tree object
        tree: String,
    }
}

#[derive(Args)]
#[group(required = true, multiple = false)]
pub(crate) struct CatFlags {
    /// pretty-print <object> content
    #[arg(short = 'p')]
    pub print_content: bool,
    /// show object type
    #[arg(short = 't')]
    pub print_type: bool,
    /// show object size
    #[arg(short = 's')]
    pub print_size: bool,
}
