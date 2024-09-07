use descord::prelude::*;

use lazy_static::lazy_static;
use nanoserde::{DeJson, SerJson};
use redis::Commands;
use dotenvy;

use tokio::sync::Mutex;

lazy_static! {
    static ref DB: Mutex<Option<redis::Connection>> = Mutex::new(None);
}

macro_rules! db {
    [] => {
        DB.lock().await.as_mut().unwrap()
    };
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().unwrap();

    let client = redis::Client::open("redis://127.0.0.1/")?;
    *DB.lock().await = Some(client.get_connection().expect("db isn't running"));

    let mut client = Client::new(
        &std::env::var("DISCORD_TOKEN").unwrap(),
        GatewayIntent::NON_PRIVILEGED | GatewayIntent::MESSAGE_CONTENT,
        "'",
    )
    .await;

    client.register_events(vec![ready(), message_create()]);
    client
        .register_slash_commands(vec![register(), get_registered(), remove_command()])
        .await;

    client.login().await;

    Ok(())
}

#[derive(DeJson, SerJson)]
struct Command {
    output: String,
    /// User id of the person who added it
    user_id: String,
    /// Number of times this command is invoked
    invocations: usize,
}

#[descord::event]
async fn ready(ready: ReadyData) {
    println!(
        "Logged in as: {}#{}",
        ready.user.username, ready.user.discriminator
    );
}

#[descord::event]
async fn message_create(msg: Message) {
    if msg.author.as_ref().unwrap().bot {
        return;
    }

    let guild_id = msg.guild_id.as_ref().unwrap();
    let cmd: Option<String> = db!().hget(guild_id, &msg.content).unwrap();

    if cmd.is_none() {
        return;
    }

    let mut cmd = Command::deserialize_json(&cmd.unwrap()).unwrap();
    cmd.invocations += 1;
    let _: () = db!()
        .hset(guild_id, &msg.content, cmd.serialize_json())
        .expect("Failed to update the value");

    msg.send_in_channel(cmd.output).await;
}

#[descord::slash(
    description = "Add a new command in this server",
    permissions = "manage_messages"
)]
async fn register(
    int: Interaction,
    /// The command for the message
    command: String,
    /// Output the bot should send when the command is invoked
    output: String,
) {
    let _: () = db!()
        .hset(
            &int.guild_id,
            command,
            Command {
                output: output.clone(),
                user_id: int
                    .member
                    .as_ref()
                    .unwrap()
                    .user
                    .as_ref()
                    .unwrap()
                    .id
                    .clone(),
                invocations: 0,
            }
            .serialize_json(),
        )
        .unwrap();

    int.reply("Command added!", false).await;
}

#[descord::slash(
    description = "Get all the registered commands on this server",
    permissions = "manage_messages"
)]
async fn get_registered(int: Interaction) {
    let list: Vec<(String, String)> = db!().hgetall(&int.guild_id).unwrap_or_default();

    if list.is_empty() {
        int.reply("No command is registered on this server :(", false)
            .await;
        return;
    }

    let mut embed = EmbedBuilder::new()
        .color(Color::Orange)
        .title("List of commands");

    for (cmd_prefix, out) in list {
        let command = Command::deserialize_json(&out).unwrap();
        embed = embed.field(
            &cmd_prefix,
            &format!(
                "Added by: {}\nInvoked {} time(s)\nOutput: {}",
                descord::utils::fetch_user(&command.user_id)
                    .await
                    .unwrap()
                    .username,
                command.invocations,
                command.output,
            ),
            false,
        );
    }

    let embed = embed.build();

    int.reply(embed, false).await;
}

#[descord::slash(description = "Remove a command", permissions = "manage_messages")]
async fn remove_command(
    int: Interaction,
    /// The command to remove
    command: String,
) {
    let command_data: Option<String> = db!().hget(&int.guild_id, command).unwrap();
    if command_data.is_none() {
        int.reply("No such command exists", true).await;
        return;
    }

    let _: () = db!()
        .hdel(&int.guild_id, command)
        .expect("Failed to delete");
    int.reply(format!("Removed `{command}` command"), false)
        .await;
}
