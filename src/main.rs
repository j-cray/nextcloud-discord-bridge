use anyhow::Context as _;
use serenity::async_trait;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use songbird::SerenityInit;
use std::env;

mod nextcloud;
mod bridge;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file if it exists
    dotenv::dotenv().ok();

    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").context("Expected a token in the environment")?;

    // Set gateway intents, which decides what events the bot will be notified about
    let intents = GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    // Create a new instance of the Client, logging in as a bot.
    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .register_songbird()
        .await
        .context("Err creating client")?;

    // Start a single shard, and start listening to events.
    println!("Starting Discord Bridge Client...");

    let songbird = client.data.read().await.get::<songbird::SongbirdKey>().unwrap().clone();

    // Spawn Discord Client
    let _client_handle = tokio::spawn(async move {
        if let Err(why) = client.start().await {
            println!("Client error: {:?}", why);
        }
    });

    // Initialize Bridge Session
    // In a real app, these would come from config or command arguments
    let guild_id = env::var("DISCORD_GUILD_ID")
        .unwrap_or("0".to_string())
        .parse::<u64>()
        .map(serenity::model::id::GuildId::new)
        .unwrap_or(serenity::model::id::GuildId::new(0));

    let channel_id = env::var("DISCORD_CHANNEL_ID")
        .unwrap_or("0".to_string())
        .parse::<u64>()
        .map(serenity::model::id::ChannelId::new)
        .unwrap_or(serenity::model::id::ChannelId::new(0));

    // Check if valid (assuming > 0 is valid)
    if guild_id.get() == 0 || channel_id.get() == 0 {
        println!("Please set DISCORD_GUILD_ID and DISCORD_CHANNEL_ID in .env");
        return Ok(());
    }

    // Initialize Nextcloud Config
    let nc_url = env::var("NEXTCLOUD_URL").context("NEXTCLOUD_URL not set")?;
    let nc_user = env::var("NEXTCLOUD_USERNAME").context("NEXTCLOUD_USERNAME not set")?;
    let nc_pass = env::var("NEXTCLOUD_PASSWORD").context("NEXTCLOUD_PASSWORD not set")?;
    let nc_room = env::var("NEXTCLOUD_ROOM_TOKEN").context("NEXTCLOUD_ROOM_TOKEN not set")?;

    println!("Initializing Nextcloud Signaling...");
    let config = nextcloud::signaling::Config {
        nextcloud_url: nc_url,
        username: nc_user,
        password: nc_pass,
    };

    let mut signaling = nextcloud::signaling::SignalingClient::new(config);
    signaling.connect(&nc_room).await.context("Failed to connect to Signaling")?;

    println!("Initializing Nextcloud WebRTC...");
    let nc_webrtc = nextcloud::webrtc::NextcloudWebRTC::new().await.context("Failed to init WebRTC")?;

    let session = bridge::BridgeSession::new(
        nc_webrtc,
        signaling,
        songbird,
        guild_id,
        channel_id
    );

    println!("Starting Bridge Session...");
    if let Err(e) = session.start().await {
         println!("Bridge Session failed: {:?}", e);
    }

    Ok(())
}
