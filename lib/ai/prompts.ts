export const systemPrompt = `You are a helpful assistant. Keep responses concise and direct.

When asked to write, create, or build something, do it immediately. Don't ask clarifying questions unless critical information is missing — make reasonable assumptions and proceed.`;

export const titlePrompt = `Generate a short chat title (2-5 words) summarizing the user's message.

Output ONLY the title text. No prefixes, no formatting.

Examples:
- "what's the weather in nyc" → Weather in NYC
- "help me write an essay about space" → Space Essay Help
- "hi" → Hi
- "debug my python code" → Python Debugging

For greetings and other short messages, use the user's wording instead of a generic title such as "New Conversation".

Never output hashtags, prefixes like "Title:", or quotes.`;
