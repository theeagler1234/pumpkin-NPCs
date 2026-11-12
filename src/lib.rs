use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use pumpkin_plugin_api::{
    Context, EntityType, Plugin, PluginMetadata, Server,
    command::{
        Arg, ArgumentType, Command, CommandError, CommandNode, CommandSender, ConsumedArgs,
        StringType,
    },
    commands::CommandHandler,
    events::{
        EventData, EventHandler, EventPriority, PlayerInteractAtEntityEvent,
        PlayerInteractEntityEvent,
    },
    events_wit::EntityInteractionAction,
    permission::{Permission, PermissionDefault, PermissionLevel},
    server::CommandSender as ServerCommandSender,
    text::TextComponent,
};
use tracing::{info, warn};

const PERMISSION: &str = "pumpkin_npcs:manage";

#[derive(Clone)]
struct Npc {
    entity_id: u32,
    world_name: String,
    command: String,
    display_name: String,
}

static NPCS: OnceLock<Mutex<HashMap<String, Npc>>> = OnceLock::new();

fn npcs() -> &'static Mutex<HashMap<String, Npc>> {
    NPCS.get_or_init(|| Mutex::new(HashMap::new()))
}

struct NpcPlugin;

impl Plugin for NpcPlugin {
    fn new() -> Self {
        Self
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "NPCs".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            authors: vec!["the_eagler123".into()],
            description: "Spawn clickable NPCs that run commands as the interacting player.".into(),
            dependencies: Vec::new(),
            permissions: Vec::new(),
        }
    }

    fn on_load(&self, context: Context) -> pumpkin_plugin_api::Result<()> {
        context.register_permission(&Permission {
            node: PERMISSION.into(),
            description: "Allows creating and editing NPCs".into(),
            default: PermissionDefault::Op(PermissionLevel::Four),
            children: Vec::new(),
        })?;

        let command = Command::new(&["npc".to_string()], "Create and edit NPCs").then(
            CommandNode::literal("spawn").then(
                CommandNode::argument("name", &ArgumentType::String(StringType::SingleWord)).then(
                    CommandNode::argument("command", &ArgumentType::String(StringType::Greedy))
                        .execute(SpawnNpc),
                ),
            ),
        );
        let command = command.then(
            CommandNode::literal("edit").then(
                CommandNode::argument("name", &ArgumentType::String(StringType::SingleWord)).then(
                    CommandNode::argument(
                        "attribute",
                        &ArgumentType::String(StringType::SingleWord),
                    )
                    .then(
                        CommandNode::argument("value", &ArgumentType::String(StringType::Greedy))
                            .execute(EditNpc),
                    ),
                ),
            ),
        );
        context.register_command(command, PERMISSION);

        context.register_event_handler(NpcInteractHandler, EventPriority::Normal, true)?;
        context.register_event_handler(NpcInteractAtHandler, EventPriority::Normal, true)?;
        info!("NPC commands and interaction handlers registered");
        Ok(())
    }

    fn on_unload(&self, context: Context) -> pumpkin_plugin_api::Result<()> {
        let server = context.get_server();
        let registered: Vec<Npc> = npcs()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values()
            .cloned()
            .collect();
        for npc in registered {
            if let Some(entity) = find_entity(&server, &npc) {
                entity.remove();
            }
        }
        npcs()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        info!("NPC entities removed");
        Ok(())
    }
}

struct SpawnNpc;

impl CommandHandler for SpawnNpc {
    fn handle(
        &self,
        sender: CommandSender,
        _server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let name = arg_string(&args, "name")?;
        let command = normalize_command(&arg_string(&args, "command")?);
        if command.is_empty() {
            return Err(command_error("The NPC command cannot be empty."));
        }
        let Some(player) = sender.as_player() else {
            return Err(command_error("Only a player can spawn an NPC."));
        };

        let key = name.to_lowercase();
        {
            let registered = npcs().lock().unwrap_or_else(|error| error.into_inner());
            if registered.contains_key(&key) {
                return Err(command_error("An NPC with that name already exists."));
            }
        }

        let world = player.get_world();
        let world_name = world.get_name();
        let entity = world.spawn_entity(EntityType::Villager, player.get_position());
        entity.set_invulnerable(true);
        entity.set_custom_name(TextComponent::text(&name));
        entity.set_custom_name_visible(true);
        if let Some(mob) = entity.as_mob() {
            mob.set_ai_disabled(true);
        }

        let mut registered = npcs().lock().unwrap_or_else(|error| error.into_inner());
        if registered.contains_key(&key) {
            return Err(command_error("An NPC with that name already exists."));
        }
        registered.insert(
            key,
            Npc {
                entity_id: entity.get_id(),
                world_name,
                command,
                display_name: name.clone(),
            },
        );

        sender.send_message(TextComponent::text(&format!("Spawned NPC '{name}'.")));
        Ok(1)
    }
}

struct EditNpc;

impl CommandHandler for EditNpc {
    fn handle(
        &self,
        sender: CommandSender,
        server: Server,
        args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        let name = arg_string(&args, "name")?;
        let attribute = arg_string(&args, "attribute")?.to_lowercase();
        let value = unquote(&arg_string(&args, "value")?);
        let key = name.to_lowercase();

        if attribute == "skin" {
            return Err(command_error(
                "Skins are not supported by Pumpkin's plugin API; NPCs are villager stand-ins.",
            ));
        }

        let mut npc = {
            let registered = npcs().lock().unwrap_or_else(|error| error.into_inner());
            registered
                .get(&key)
                .cloned()
                .ok_or_else(|| command_error("No NPC with that name exists."))?
        };

        match attribute.as_str() {
            "name" => {
                if value.is_empty() {
                    return Err(command_error("The NPC display name cannot be empty."));
                }
                npc.display_name = value;
                let entity = find_entity(&server, &npc)
                    .ok_or_else(|| command_error("The NPC entity could not be found."))?;
                entity.set_custom_name(TextComponent::text(&npc.display_name));
                entity.set_custom_name_visible(true);
            }
            "command" => {
                let command = normalize_command(&value);
                if command.is_empty() {
                    return Err(command_error("The NPC command cannot be empty."));
                }
                npc.command = command;
            }
            _ => {
                return Err(command_error(
                    "Unknown attribute. Use 'name', 'command', or 'skin'.",
                ));
            }
        }

        npcs()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(key, npc);
        sender.send_message(TextComponent::text(&format!(
            "Updated NPC '{name}' ({attribute})."
        )));
        Ok(1)
    }
}

struct NpcInteractHandler;

impl EventHandler<PlayerInteractEntityEvent> for NpcInteractHandler {
    fn handle(
        &self,
        server: Server,
        mut event: EventData<PlayerInteractEntityEvent>,
    ) -> EventData<PlayerInteractEntityEvent> {
        let world_name = event.player.get_world().get_name();
        if let Some(command) = command_for_entity(&world_name, event.entity_id as u32) {
            if matches!(
                event.action,
                EntityInteractionAction::Interact | EntityInteractionAction::Attack
            ) {
                run_as_player(&server, event.player.get_id(), &command);
                event.cancelled = true;
            }
        }
        event
    }
}

struct NpcInteractAtHandler;

impl EventHandler<PlayerInteractAtEntityEvent> for NpcInteractAtHandler {
    fn handle(
        &self,
        server: Server,
        mut event: EventData<PlayerInteractAtEntityEvent>,
    ) -> EventData<PlayerInteractAtEntityEvent> {
        let world_name = event.player.get_world().get_name();
        if let Some(command) = command_for_entity(&world_name, event.entity_id as u32) {
            run_as_player(&server, event.player.get_id(), &command);
            event.cancelled = true;
        }
        event
    }
}

fn command_for_entity(world_name: &str, entity_id: u32) -> Option<String> {
    npcs()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .values()
        .find(|npc| npc.world_name == world_name && npc.entity_id == entity_id)
        .map(|npc| npc.command.clone())
}

fn run_as_player(server: &Server, player_id: pumpkin_plugin_api::uuid::Uuid, command: &str) {
    if let Some(player) = server.get_player_by_uuid(player_id) {
        server.execute_command(command, ServerCommandSender::Player(player));
    } else {
        warn!("Could not execute NPC command because the interacting player left");
    }
}

fn find_entity(server: &Server, npc: &Npc) -> Option<pumpkin_plugin_api::Entity> {
    let world = server.get_world_by_name(&npc.world_name)?;
    world
        .get_entities()
        .into_iter()
        .find(|entity| entity.get_id() == npc.entity_id)
}

fn arg_string(args: &ConsumedArgs, key: &str) -> Result<String, CommandError> {
    match args.get_value(key) {
        Arg::Simple(value) | Arg::Msg(value) => Ok(value),
        _ => Err(command_error("A text argument was expected.")),
    }
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
        .to_string()
}

fn normalize_command(command: &str) -> String {
    unquote(command).trim().trim_start_matches('/').to_string()
}

fn command_error(message: &str) -> CommandError {
    CommandError::CommandFailed(TextComponent::text(message))
}

pumpkin_plugin_api::register_plugin!(NpcPlugin);

#[cfg(test)]
mod tests {
    use super::{normalize_command, unquote};

    #[test]
    fn normalizes_quoted_slash_command() {
        assert_eq!(normalize_command("\"/server survival\""), "server survival");
    }

    #[test]
    fn leaves_unquoted_command_arguments_intact() {
        assert_eq!(normalize_command("/say hello world"), "say hello world");
    }

    #[test]
    fn removes_wrapping_quotes_from_display_name() {
        assert_eq!(unquote("\"CLOSED\""), "CLOSED");
    }
}
