use std::{collections::HashMap, env};

use serenity::{
    async_trait,
    builder::CreateEmbed,
    model::{
        channel::{Message, Reaction, ReactionType},
        gateway::Ready,
        id::ChannelId,
        Permissions,
    },
    prelude::*,
    utils::MessageBuilder,
};

const SKULL: &str = "💀";
const ULTRA_SKULL: &str = "☠️";

struct SkullBoarder {
    skull_count: Mutex<u64>,
    skull_boards_msgs: Mutex<HashMap<u64, Message>>,
    channel: Mutex<Option<ChannelId>>,
}

impl SkullBoarder {
    async fn handle_reaction_change(&self, ctx: Context, reaction: Reaction) {
        if !reaction.emoji.unicode_eq(SKULL) {
            return;
        }

        let channel = if let Some(channel) = *self.channel.lock().await {
            channel
        } else {
            return;
        };

        let msg = match reaction.message(&ctx.http).await {
            Ok(msg) => msg,
            Err(why) => {
                println!("Error fetching message: {:?}", why);
                return;
            }
        };

        if matches!(reaction.user_id, Some(uid) if uid == msg.author.id) {
            return;
        }

        let mut me = 0;

        match msg
            .reaction_users(
                &ctx.http,
                ReactionType::Unicode(SKULL.to_string()),
                Some(1),
                None,
            )
            .await
        {
            Ok(mut users) => {
                if let Some(next) = users.pop() {
                    let after = match msg
                        .reaction_users(
                            &ctx.http,
                            ReactionType::Unicode(SKULL.to_string()),
                            Some(1),
                            Some(msg.author.id),
                        )
                        .await
                    {
                        Ok(mut users) => users.pop(),
                        Err(why) => {
                            println!("Error fetching users: {:?}", why);
                            return;
                        }
                    };

                    me = match after {
                        Some(ref user) if *user != next => 1,
                        None => 1,
                        _ => 0,
                    };
                }
            }
            Err(why) => {
                println!("Error fetching users: {:?}", why);
                return;
            }
        };

        let count = *self.skull_count.lock().await;

        let reaction = if let Some(reaction) = msg
            .reactions
            .iter()
            .find(|r| r.reaction_type.unicode_eq(SKULL) && r.count - me >= count)
        {
            reaction
        } else {
            if let Some(message) = self.skull_boards_msgs.lock().await.remove(msg.id.as_u64()) {
                if let Err(why) = message.delete(&ctx.http).await {
                    println!("Error deleting message: {:?}", why);
                }
            }
            return;
        };

        let new_msg = MessageBuilder::new()
            .push(SKULL)
            .push(" **")
            .push(reaction.count - me)
            .push(" |** ")
            .channel(msg.channel_id)
            .push('\n')
            .build();

        let mut msgs = self.skull_boards_msgs.lock().await;

        if let Some(message) = msgs.get_mut(msg.id.as_u64()) {
            if let Err(why) = message
                .edit(&ctx.http, |m| {
                    m.content(new_msg).embed(|e| create_embed(e, &msg))
                })
                .await
            {
                println!("Error editing message: {:?}", why);
            }
        } else {
            match channel
                .send_message(&ctx.http, |m| {
                    m.content(&new_msg).embed(|e| create_embed(e, &msg))
                })
                .await
            {
                Ok(new_msg) => {
                    let _ = new_msg
                        .react(&ctx.http, ReactionType::Unicode(ULTRA_SKULL.to_string()))
                        .await;
                    msgs.insert(*msg.id.as_u64(), new_msg);
                }
                Err(why) => {
                    println!("Error sending message: {:?}", why);
                }
            }
        }
    }
}

fn create_embed<'a>(e: &'a mut CreateEmbed, msg: &Message) -> &'a mut CreateEmbed {
    e.author(|a| {
        a.name(&msg.author.name)
            .url("https://github.com/Bunch-of-cells")
            .icon_url(
                msg.author
                    .avatar_url()
                    .as_deref()
                    .unwrap_or("https://cdn.discordapp.com/embed/avatars/0.png"),
            )
    })
    .description(format!(
        "{}\n\n\n[Go to Message]({})",
        &msg.content,
        msg.link()
    ))
    .color(0xFF0000);

    for attachment in msg.attachments.iter().filter(|a| a.height.is_some()) {
        e.image(&attachment.url);
    }
    e
}

#[async_trait]
impl EventHandler for SkullBoarder {
    async fn message(&self, ctx: Context, msg: Message) {
        if let Some(("!setskull", times)) = msg.content.split_once(char::is_whitespace) {
            if let Some(member) = &msg.member {
                for role in &member.roles {
                    if role
                        .to_role_cached(&ctx.cache)
                        .await
                        .map_or(false, |r| r.has_permission(Permissions::ADMINISTRATOR))
                    {
                        if let Ok(times) = times.parse::<u64>() {
                            *self.skull_count.lock().await = times;
                            let _ = msg.channel_id.say(&ctx.http, "Skull count set!").await;
                        } else {
                            let _ = msg.channel_id.say(&ctx.http, "Invalid number!").await;
                        }
                    }
                }
            }
        } else if msg.content == "!setchannel" {
            if let Some(member) = &msg.member {
                for role in &member.roles {
                    if role
                        .to_role_cached(&ctx.cache)
                        .await
                        .map_or(false, |r| r.has_permission(Permissions::ADMINISTRATOR))
                    {
                        *self.channel.lock().await = Some(msg.channel_id);
                        let _ = msg.channel_id.say(&ctx.http, "Channel set!").await;
                    }
                }
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }

    async fn reaction_add(&self, ctx: Context, reaction: Reaction) {
        self.handle_reaction_change(ctx, reaction).await;
    }

    async fn reaction_remove(&self, ctx: Context, reaction: Reaction) {
        self.handle_reaction_change(ctx, reaction).await;
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().expect("Failed to load .env file");
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let mut client = Client::builder(&token)
        .event_handler(SkullBoarder {
            skull_count: Mutex::new(4),
            skull_boards_msgs: Mutex::new(HashMap::new()),
            channel: Mutex::new(None),
        })
        .await
        .expect("Err creating client");

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
