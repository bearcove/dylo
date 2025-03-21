use crate::{
    codegen::codegen_mod,
    dependency::{add_dependency, remove_dependency},
    types::{DyloCommand, Scope},
    workspace::{get_single_mod, list_mods},
};

pub fn run_command(workspace_root: camino::Utf8PathBuf, command: DyloCommand) -> eyre::Result<()> {
    match command {
        DyloCommand::Default { scope, force } => {
            for mod_info in list_mods(&workspace_root, scope)? {
                codegen_mod(mod_info, force)?;
            }
        }
        DyloCommand::List { scope } => {
            let mods = list_mods(&workspace_root, scope)?;
            if mods.is_empty() {
                eprintln!("No dylo modules found in {workspace_root}.");
                eprintln!("dylo looks for any Cargo workspace item that starts with `mod-`");
            } else {
                for mod_info in mods {
                    println!("{}", mod_info.name);
                }
            }
        }
        DyloCommand::Add {
            is_impl,
            deps,
            scope,
        } => {
            let mod_info = get_single_mod(&workspace_root, scope)?;

            tracing::info!(
                "Adding dependencies to mod '{name}': {deps_joined}{is_impl_suffix}",
                name = mod_info.name,
                deps_joined = deps.join(", "),
                is_impl_suffix = if is_impl { " (impl-only)" } else { "" }
            );

            add_dependency(&mod_info.mod_path, &deps, is_impl)?;
            tracing::info!("✅ Dependencies added successfully");
        }
        DyloCommand::Rm { deps, scope } => {
            let mod_info = get_single_mod(&workspace_root, scope)?;

            tracing::info!(
                "Removing dependencies from mod '{name}': {}",
                deps.join(", "),
                name = mod_info.name
            );

            remove_dependency(&mod_info.mod_path, &deps)?;
            tracing::info!("✅ Dependencies removed successfully");
        }
    }
    Ok(())
}

pub fn parse_args(ambient_scope: Scope) -> DyloCommand {
    let cli = clap::Command::new("dylo")
        .about("Dynamic loading utility for Rust")
        .subcommand_required(true)
        .subcommand(
            clap::Command::new("gen")
                .about("Generate consumer crates from mod implementations")
                .arg(
                    clap::Arg::new("force")
                        .short('f')
                        .long("force")
                        .help("Force regeneration of all consumer crates")
                        .action(clap::ArgAction::SetTrue),
                )
                .arg(
                    clap::Arg::new("mod")
                        .short('m')
                        .long("mod")
                        .help("Restrict processing to a specific mod")
                        .value_name("NAME"),
                )
                .arg(
                    clap::Arg::new("workspace")
                        .short('w')
                        .long("workspace")
                        .help("Process all mods in the workspace (opposite of --mod)")
                        .action(clap::ArgAction::SetTrue),
                ),
        )
        .subcommand(
            clap::Command::new("add")
                .about("Add dependencies to a mod")
                .arg(
                    clap::Arg::new("mod")
                        .short('m')
                        .long("mod")
                        .help("Specify the mod to process")
                        .value_name("NAME"),
                )
                .arg(
                    clap::Arg::new("impl")
                        .short('i')
                        .long("impl")
                        .help("Mark dependencies as impl-only")
                        .action(clap::ArgAction::SetTrue),
                )
                .arg(
                    clap::Arg::new("deps")
                        .help("Dependencies to add")
                        .required(true)
                        .num_args(1..)
                        .value_parser(clap::value_parser!(String)),
                ),
        )
        .subcommand(
            clap::Command::new("rm")
                .about("Remove dependencies from a mod")
                .arg(
                    clap::Arg::new("mod")
                        .short('m')
                        .long("mod")
                        .help("Specify the mod to process")
                        .value_name("NAME"),
                )
                .arg(
                    clap::Arg::new("deps")
                        .help("Dependencies to remove")
                        .required(true)
                        .num_args(1..)
                        .value_parser(clap::value_parser!(String)),
                ),
        )
        .subcommand(
            clap::Command::new("list").about("List available mods").arg(
                clap::Arg::new("workspace")
                    .short('w')
                    .long("workspace")
                    .help("List all mods in the workspace")
                    .action(clap::ArgAction::SetTrue),
            ),
        );

    let matches = cli.get_matches();

    fn get_module_scope(
        matches: &clap::ArgMatches,
        ambient_scope: &Scope,
    ) -> Result<Scope, &'static str> {
        match matches.get_one::<String>("mod").cloned() {
            Some(name) => Ok(Scope::Module(name)),
            None => match ambient_scope {
                Scope::Module(name) => Ok(Scope::Module(name.clone())),
                Scope::Workspace => {
                    Err("Must specify a module with --mod or run from a module directory")
                }
            },
        }
    }

    match matches.subcommand() {
        Some(("gen", sub_matches)) => {
            tracing::debug!("Processing 'gen' subcommand");
            let force = sub_matches.get_flag("force");
            tracing::debug!("Force flag: {force}");

            let scope = match sub_matches.get_one::<String>("mod").cloned() {
                Some(name) => {
                    tracing::debug!("Module explicitly specified: {name}");
                    Scope::Module(name)
                }
                None => {
                    tracing::debug!("No module explicitly specified");
                    if sub_matches.get_flag("workspace") {
                        tracing::debug!("Workspace flag is set, using workspace scope");
                        Scope::Workspace
                    } else {
                        tracing::debug!("Using ambient scope: {ambient_scope:?}");
                        if let Scope::Module(mod_name) = &ambient_scope {
                            tracing::info!("Operating in module scope: {mod_name}");
                            tracing::info!("Use --workspace to operate on all packages instead");
                        }
                        ambient_scope
                    }
                }
            };
            tracing::debug!("Final scope determined: {scope:?}");

            DyloCommand::Default { force, scope }
        }

        Some(("add", sub_matches)) => {
            let scope = get_module_scope(sub_matches, &ambient_scope).unwrap_or_else(|err| {
                eprintln!("Error: {err}");
                std::process::exit(1);
            });

            DyloCommand::Add {
                scope,
                is_impl: sub_matches.get_flag("impl"),
                deps: sub_matches
                    .get_many::<String>("deps")
                    .unwrap()
                    .cloned()
                    .collect(),
            }
        }

        Some(("rm", sub_matches)) => {
            let scope = get_module_scope(sub_matches, &ambient_scope).unwrap_or_else(|err| {
                eprintln!("Error: {err}");
                std::process::exit(1);
            });

            DyloCommand::Rm {
                scope,
                deps: sub_matches
                    .get_many::<String>("deps")
                    .unwrap()
                    .cloned()
                    .collect(),
            }
        }

        Some(("list", sub_matches)) => {
            let scope = if sub_matches.get_flag("workspace") {
                Scope::Workspace
            } else {
                ambient_scope
            };

            DyloCommand::List { scope }
        }

        _ => unreachable!("clap ensures we have a valid subcommand"),
    }
}
