use serenity::{
    all::{EmojiId, ReactionType}, async_trait, model::{channel::Message, gateway::Ready}, prelude::*
};
use tracing::{error, info};

struct Handler;

/// 「おお」「オオ」「oo」（大文字小文字問わず）が何個あるか数える
/// 例: "おおおおおお" → 3, "oooo" → 2, "おおoo" → 2
fn count_oo(s: &str) -> usize {
    let mut count = 0;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i + 1 < chars.len() {
        let (a, b) = (chars[i], chars[i + 1]);
        if (a == 'お' && b == 'お')
            || (a == 'オ' && b == 'オ')
            || (a.to_ascii_lowercase() == 'o' && b.to_ascii_lowercase() == 'o')
        {
            count += 1;
            i += 2; // 消費した2文字分スキップ（おおおお → 2個、おおお → 1個）
        } else {
            i += 1;
        }
    }
    count
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }

        let stamp = "<:Omilfy:1489695886773587978>";
        let count = count_oo(&msg.content);

        if msg.content.contains("これはおお") {
            if let Err(e) = msg.channel_id.say(&ctx.http, stamp).await {
                error!("メッセージ送信エラー: {:?}", e);
            }
        } else if count > 0 {
            let send_msg = (0..count)
                .map(|_| stamp)
                .collect::<Vec<_>>()
                .join(" ");

            if count == 1 {
                let emoji = ReactionType::Custom {
                    animated: false,                           // アニメ絵文字なら true
                    id: EmojiId::new(1489695886773587978),      // ← 絵文字IDに変更
                    name: Some("Omilfy".to_string()), // ← 絵文字名に変更
                };

                if let Err(e) = msg.react(&ctx.http, emoji).await {
                    error!("リアクション追加エラー: {:?}", e);
                }
            } else {
                if let Err(e) = msg.channel_id.say(&ctx.http, send_msg).await {
                    error!("メッセージ送信エラー: {:?}", e);
                }
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        info!("{}としてログインしました", ready.user.name);
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    dotenvy::dotenv().ok();
    let token = std::env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN が .env に設定されていません");

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await
        .expect("クライアントの作成に失敗しました");

    if let Err(e) = client.start().await {
        error!("クライアントエラー: {:?}", e);
    }
}