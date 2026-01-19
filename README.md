# Nextcloud Discord Bridge

<div align="center">

**Unify Your Chat**

[![Nextcloud](https://img.shields.io/badge/Platform-Nextcloud-blue?style=for-the-badge&logo=nextcloud)]()
[![Discord](https://img.shields.io/badge/Platform-Discord-5865F2?style=for-the-badge&logo=discord)]()

*Bi-directional syncing between Nextcloud Talk and Discord channels.*

</div>

---

## 📖 Overview
This project acts as a bridge between a Nextcloud Talk conversation and a Discord channel. It relays messages, images, and basic events back and forth, allowing teams to communicate seamlessly regardless of which platform they prefer.

## ⚙️ How It Works
1.  **Discord -> Talk:** A Discord bot listens for messages and uses the Nextcloud Talk API to repost them.
2.  **Talk -> Discord:** Nextcloud Talk webhooks (or polling) trigger the bot to post to Discord.

---

## 🗺️ Roadmap & Todo

### Phase 1: Discord Bot
- [ ] **Setup:** Create the Discord Application and Bot User.
- [ ] **Listeners:** Implement `messageCreate` event handler.
- [ ] **Formatting:** Convert Discord Markdown to Nextcloud Talk format.

### Phase 2: Nextcloud Integration
- [ ] **API Client:** Authenticate with Nextcloud Talk (using a dedicated bot user).
- [ ] **Polling/Webhooks:** Implement the mechanism to detect new Talk messages.
- [ ] **User Mapping:** (Optional) Map Discord User IDs to Nextcloud Usernames for cleaner attribution.

### Phase 3: Media Handling
- [ ] **Images:** Handle downloading images from one platform and re-uploading to the other.
- [ ] **Files:** Link or transfer smaller file attachments.

### Phase 4: Polish
- [ ] **Commands:** Add `!status` or `!sync` commands to the Discord bot.
- [ ] **Docker:** Dockerize the bridge for 24/7 uptime.
