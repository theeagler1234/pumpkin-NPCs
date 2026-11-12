# Pumpkin NPCs

A Pumpkin Minecraft plugin that adds named, clickable villager stand-ins. Right-clicking or punching a managed NPC runs its configured command as the interacting player, so the command uses that player's normal permissions.

> **API limitation:** Pumpkin's current plugin API does not expose the player-profile hooks needed to spawn fake player entities or apply Java Edition skins. NPCs are therefore immobile, invulnerable villagers. Setting a `skin` attribute returns an unsupported-feature message.

## Build

Install the Rust WebAssembly target if needed:

```sh
rustup target add wasm32-wasip2
```

Build a release plugin from this directory:

```sh
cargo build --release
```

The resulting plugin is:

```text
target/wasm32-wasip2/release/pumpkin_npcs.wasm
```

Copy the `.wasm` file into your Pumpkin server's `plugins/` directory and start or reload the server.

## Commands

NPC management commands require the `pumpkin_npcs:manage` permission, which defaults to server operators at permission level 4.

Spawn an NPC at your current position with a command to run on interaction:

```text
/npc spawn survival "/server survival"
```

Edit its displayed name:

```text
/npc edit survival name "CLOSED"
```

Change the command it runs:

```text
/npc edit survival command "/server survival"
```

The name supplied to `spawn` is the NPC's lookup name for later edits. Changing its displayed name does not change that lookup name. Command values may be quoted and may include a leading `/`; both are removed before execution.

## Persistence

NPC registrations are held in memory. They are removed when the plugin unloads, and are not saved across server restarts.
