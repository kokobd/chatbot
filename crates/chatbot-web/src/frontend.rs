use leptos::prelude::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <main class="chatbot-shell">
                <header class="chatbot-header">
                    <a href="/" class="brand">"Chatbot"</a>
                    <a href="/" class="new-chat">"New chat"</a>
                </header>
                <Routes fallback=|| view! { <NotFound/> }>
                    <Route path=path!("") view=Home/>
                    <Route path=path!("/chat/:id") view=Chat/>
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn Home() -> impl IntoView {
    view! {
        <section class="chat-page">
            <h1>"What would you like to explore?"</h1>
            <p>"Start a private conversation with a curated model."</p>
            <a class="primary-action" href="/chat/new">"Start a chat"</a>
        </section>
    }
}

#[component]
fn Chat() -> impl IntoView {
    view! {
        <section class="chat-page">
            <div class="messages" aria-live="polite">
                <p class="empty-state">"Your conversation will appear here."</p>
            </div>
            <form class="composer" action="#" method="post">
                <label for="message">"Message"</label>
                <textarea id="message" name="message" placeholder="Ask anything..."></textarea>
                <button type="submit">"Send"</button>
            </form>
        </section>
    }
}

#[component]
fn NotFound() -> impl IntoView {
    view! { <section class="chat-page"><h1>"Not found"</h1></section> }
}
