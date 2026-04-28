use leptos::prelude::*;

use crate::server::api::{create_session, delete_session, list_sessions, rename_session};
use crate::types::*;

/// Formats a Unix timestamp (seconds) into a human-readable relative time string.
fn relative_time(timestamp_secs: i64) -> String {
    cfg_if::cfg_if! {
        if #[cfg(feature = "ssr")] {
            let now = chrono::Utc::now().timestamp();
        } else if #[cfg(feature = "hydrate")] {
            let now = (js_sys::Date::now() / 1000.0) as i64;
        } else {
            let now = 0i64;
        }
    }

    let diff = now - timestamp_secs;
    if diff < 0 {
        return "just now".into();
    }

    let minutes = diff / 60;
    let hours = minutes / 60;
    let days = hours / 24;

    if minutes < 1 {
        "just now".into()
    } else if minutes < 60 {
        format!("{}m ago", minutes)
    } else if hours < 24 {
        format!("{}h ago", hours)
    } else if days == 1 {
        "yesterday".into()
    } else if days < 7 {
        format!("{}d ago", days)
    } else {
        format!("{}w ago", days / 7)
    }
}

/// A single session row in the sidebar with inline rename and delete confirmation.
#[component]
fn SessionRow(
    session: Session,
    is_active: Signal<bool>,
    on_click: Callback<Session>,
    /// Which session ID is currently being renamed (shared signal).
    renaming_id: ReadSignal<Option<String>>,
    set_renaming_id: WriteSignal<Option<String>>,
    /// Which session ID is in "confirm delete" mode (shared signal).
    confirming_delete_id: ReadSignal<Option<String>>,
    set_confirming_delete_id: WriteSignal<Option<String>>,
    /// Bumps the sessions version to re-fetch the list.
    set_sessions_version: WriteSignal<u64>,
    /// The active session id (so we can clear it on delete).
    set_active_session_id: WriteSignal<Option<String>>,
    active_session_id: ReadSignal<Option<String>>,
) -> impl IntoView {
    let session_id = session.id.clone();
    let session_for_click = session.clone();
    let model_name = session.model_name.clone();
    let preview_text = if session.preview.is_empty() {
        "No messages yet".to_string()
    } else {
        session.preview.clone()
    };
    let time_text = relative_time(session.updated_at);

    // Local signal for the rename text input value.
    let (rename_value, set_rename_value) = signal(preview_text.clone());

    // -- Rename logic --
    let sid_for_rename_check = session_id.clone();
    let is_renaming =
        Signal::derive(move || renaming_id.get().as_deref() == Some(sid_for_rename_check.as_str()));

    // Start rename on double-click.
    let sid_for_dblclick = session_id.clone();
    let preview_for_dblclick = preview_text.clone();
    let on_double_click = move |_: leptos::ev::MouseEvent| {
        set_rename_value.set(preview_for_dblclick.clone());
        set_renaming_id.set(Some(sid_for_dblclick.clone()));
    };

    // Commit rename.
    let sid_for_commit = session_id.clone();
    let commit_rename = move || {
        let new_name = rename_value.get_untracked().trim().to_string();
        if new_name.is_empty() {
            set_renaming_id.set(None);
            return;
        }
        set_renaming_id.set(None);
        let sid = sid_for_commit.clone();
        leptos::task::spawn_local(async move {
            if rename_session(sid, new_name).await.is_ok() {
                set_sessions_version.update(|v| *v += 1);
            }
        });
    };

    // Cancel rename.
    let cancel_rename = move || {
        set_renaming_id.set(None);
    };

    // Keydown handler for rename input.
    let commit_rename_for_key = commit_rename.clone();
    let on_rename_keydown = move |ev: leptos::ev::KeyboardEvent| {
        let key = ev.key();
        if key == "Enter" {
            ev.prevent_default();
            commit_rename_for_key();
        } else if key == "Escape" {
            ev.prevent_default();
            cancel_rename();
        }
    };

    // Blur handler commits rename.
    let commit_rename_for_blur = commit_rename.clone();
    let on_rename_blur = move |_: leptos::ev::FocusEvent| {
        commit_rename_for_blur();
    };

    // -- Delete confirmation logic --
    let sid_for_delete_check = session_id.clone();
    let is_confirming_delete = Signal::derive(move || {
        confirming_delete_id.get().as_deref() == Some(sid_for_delete_check.as_str())
    });

    let sid_for_delete = session_id.clone();
    let on_delete_click = move |ev: leptos::ev::MouseEvent| {
        ev.stop_propagation();
        if is_confirming_delete.get_untracked() {
            // Second click: perform deletion.
            let sid = sid_for_delete.clone();
            set_confirming_delete_id.set(None);
            leptos::task::spawn_local(async move {
                if delete_session(sid.clone()).await.is_ok() {
                    // If we deleted the active session, clear it.
                    if active_session_id.get_untracked().as_deref() == Some(sid.as_str()) {
                        set_active_session_id.set(None);
                    }
                    set_sessions_version.update(|v| *v += 1);
                }
            });
        } else {
            // First click: enter confirm state.
            let sid = session_id.clone();
            set_confirming_delete_id.set(Some(sid.clone()));

            // Auto-revert after 3 seconds.
            cfg_if::cfg_if! {
                if #[cfg(feature = "hydrate")] {
                    let sid_for_timeout = sid;
                    gloo_timers::callback::Timeout::new(3_000, move || {
                        // Only clear if still confirming the same session.
                        if confirming_delete_id.get_untracked().as_deref()
                            == Some(sid_for_timeout.as_str())
                        {
                            set_confirming_delete_id.set(None);
                        }
                    })
                    .forget();
                }
            }
        }
    };

    view! {
        <div class="relative group">
            <button
                class=move || format!(
                    "w-full text-left px-3 py-2.5 rounded-lg transition-colors duration-100 group {}",
                    if is_active.get() {
                        "sidebar-session-active bg-zinc-800/60"
                    } else {
                        "hover:bg-zinc-800/30"
                    }
                )
                on:click=move |_| on_click.run(session_for_click.clone())
                on:dblclick=on_double_click
            >
                <div class="flex items-start justify-between gap-2">
                    <div class="min-w-0 flex-1">
                        <p class="text-[11px] text-zinc-500 font-medium truncate">{model_name}</p>
                        {move || {
                            if is_renaming.get() {
                                view! {
                                    <input
                                        type="text"
                                        class="w-full text-sm text-zinc-200 bg-zinc-800 border border-zinc-700 rounded px-1 py-0.5 mt-0.5 outline-none focus:border-amber-500/50"
                                        prop:value=move || rename_value.get()
                                        on:input=move |ev| {
                                            set_rename_value.set(event_target_value(&ev));
                                        }
                                        on:keydown=on_rename_keydown.clone()
                                        on:blur=on_rename_blur.clone()
                                        on:click=|ev: leptos::ev::MouseEvent| ev.stop_propagation()
                                        // Auto-focus when shown
                                        node_ref={
                                            let node_ref = NodeRef::<leptos::html::Input>::new();
                                            Effect::new(move || {
                                                if let Some(el) = node_ref.get() {
                                                    let _ = el.focus();
                                                    el.select();
                                                }
                                            });
                                            node_ref
                                        }
                                    />
                                }.into_any()
                            } else {
                                view! {
                                    <p class="text-sm text-zinc-300 truncate mt-0.5 group-hover:text-zinc-200 transition-colors">
                                        {preview_text.clone()}
                                    </p>
                                }.into_any()
                            }
                        }}
                    </div>
                    <span class="text-[11px] text-zinc-600 whitespace-nowrap pt-0.5 flex-shrink-0">{time_text}</span>
                </div>
            </button>

            // Delete button — visible on hover or when confirming
            <button
                class=move || format!(
                    "absolute top-1.5 right-1 p-1 rounded transition-colors {}",
                    if is_confirming_delete.get() {
                        "text-red-400 bg-red-500/10 opacity-100"
                    } else {
                        "text-zinc-600 hover:text-zinc-400 hover:bg-zinc-800/50 opacity-0 group-hover:opacity-100"
                    }
                )
                title=move || if is_confirming_delete.get() { "Click again to delete" } else { "Delete session" }
                on:click=on_delete_click
            >
                {move || {
                    if is_confirming_delete.get() {
                        view! {
                            <span class="text-[10px] font-medium px-0.5">"Delete?"</span>
                        }.into_any()
                    } else {
                        view! {
                            <svg xmlns="http://www.w3.org/2000/svg" class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/>
                            </svg>
                        }.into_any()
                    }
                }}
            </button>
        </div>
    }
}

/// The left sidebar showing session history and a button to create new sessions.
#[component]
pub fn SessionSidebar() -> impl IntoView {
    let active_model =
        use_context::<ReadSignal<Option<ModelInfo>>>().expect("active_model context");
    let _set_active_model =
        use_context::<WriteSignal<Option<ModelInfo>>>().expect("set_active_model context");
    let active_session_id =
        use_context::<ReadSignal<Option<String>>>().expect("active_session_id context");
    let set_active_session_id =
        use_context::<WriteSignal<Option<String>>>().expect("set_active_session_id context");

    let (sessions_version, set_sessions_version) = signal(0u64);
    let (creating, set_creating) = signal(false);

    // Shared signals for rename and delete confirmation state.
    let (renaming_id, set_renaming_id) = signal::<Option<String>>(None);
    let (confirming_delete_id, set_confirming_delete_id) = signal::<Option<String>>(None);

    // Search/filter state.
    let (search_query, set_search_query) = signal(String::new());

    // Fetch sessions (re-fetch when version bumps)
    let sessions = Resource::new(
        move || sessions_version.get(),
        |_| async move { list_sessions().await.unwrap_or_default() },
    );

    // Select a session
    let on_select = Callback::new(move |session: Session| {
        set_active_session_id.set(Some(session.id.clone()));
        // Also set the model info for the session (we reconstruct a minimal ModelInfo)
        // In a real app we would fetch the full ModelInfo, but for now we just set the name
        // The ChatView will reload messages based on session_id
        let _ = session;
    });

    // Create a new session
    let on_new_session = move |_: leptos::ev::MouseEvent| {
        let model = match active_model.get() {
            Some(m) => m,
            None => return,
        };

        set_creating.set(true);
        let model_id = model.id.clone();

        leptos::task::spawn_local(async move {
            match create_session(model_id).await {
                Ok(session) => {
                    set_active_session_id.set(Some(session.id));
                    set_sessions_version.update(|v| *v += 1);
                }
                Err(_) => {
                    // Session creation failed -- silently handled
                }
            }
            set_creating.set(false);
        });
    };

    view! {
        <aside class="w-64 bg-zinc-900/30 border-r border-zinc-800/40 flex flex-col h-full">
            // Header
            <div class="flex items-center justify-between px-4 py-3 border-b border-zinc-800/40">
                <h2 class="text-xs font-semibold uppercase tracking-wider text-zinc-500">"Sessions"</h2>
                <button
                    class="p-1 rounded-md text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50 transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
                    title="New session"
                    disabled=move || active_model.get().is_none() || creating.get()
                    on:click=on_new_session
                >
                    {move || {
                        if creating.get() {
                            view! {
                                <div class="w-4 h-4 border-2 border-zinc-700 border-t-amber-500 rounded-full animate-spin"></div>
                            }.into_any()
                        } else {
                            view! {
                                <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 4.5v15m7.5-7.5h-15"/>
                                </svg>
                            }.into_any()
                        }
                    }}
                </button>
            </div>

            // Search input
            <div class="px-2 pt-2">
                <div class="relative">
                    <svg
                        xmlns="http://www.w3.org/2000/svg"
                        class="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-zinc-600 pointer-events-none"
                        fill="none"
                        viewBox="0 0 24 24"
                        stroke="currentColor"
                        stroke-width="2"
                    >
                        <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z"/>
                    </svg>
                    <input
                        type="text"
                        placeholder="Search sessions..."
                        class="w-full pl-8 pr-3 py-1.5 bg-zinc-900 border border-zinc-800 rounded-lg text-sm text-zinc-300 placeholder-zinc-600 outline-none focus:border-zinc-700 transition-colors"
                        prop:value=move || search_query.get()
                        on:input=move |ev| {
                            set_search_query.set(event_target_value(&ev));
                        }
                    />
                </div>
            </div>

            // Session list
            <div class="flex-1 overflow-y-auto px-2 py-2 space-y-0.5">
                <Suspense fallback=move || view! {
                    <div class="flex items-center justify-center py-8">
                        <div class="w-4 h-4 border-2 border-zinc-700 border-t-amber-500 rounded-full animate-spin"></div>
                    </div>
                }>
                    {move || {
                        let all_sessions = sessions.get().unwrap_or_default();
                        let query = search_query.get().to_lowercase();

                        let session_list: Vec<Session> = if query.is_empty() {
                            all_sessions
                        } else {
                            all_sessions
                                .into_iter()
                                .filter(|s| s.preview.to_lowercase().contains(&query))
                                .collect()
                        };

                        if session_list.is_empty() {
                            if !query.is_empty() {
                                // No results for search
                                view! {
                                    <div class="flex flex-col items-center justify-center py-8 px-4 text-center">
                                        <p class="text-xs text-zinc-600">"No matching sessions"</p>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="flex flex-col items-center justify-center py-12 px-4 text-center">
                                        <svg xmlns="http://www.w3.org/2000/svg" class="w-8 h-8 text-zinc-800 mb-2" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1">
                                            <path stroke-linecap="round" stroke-linejoin="round" d="M20.25 8.511c.884.284 1.5 1.128 1.5 2.097v4.286c0 1.136-.847 2.1-1.98 2.193-.34.027-.68.052-1.02.072v3.091l-3-3c-1.354 0-2.694-.055-4.02-.163a2.115 2.115 0 01-.825-.242m9.345-8.334a2.126 2.126 0 00-.476-.095 48.64 48.64 0 00-8.048 0c-1.131.094-1.976 1.057-1.976 2.192v4.286c0 .837.46 1.58 1.155 1.951m9.345-8.334V6.637c0-1.621-1.152-3.026-2.76-3.235A48.455 48.455 0 0011.25 3c-2.115 0-4.198.137-6.24.402-1.608.209-2.76 1.614-2.76 3.235v6.226c0 1.621 1.152 3.026 2.76 3.235.577.075 1.157.14 1.74.194V21l4.155-4.155"/>
                                        </svg>
                                        <p class="text-xs text-zinc-600">"No sessions yet"</p>
                                        {move || active_model.get().map(|_| view! {
                                            <p class="text-[11px] text-zinc-700 mt-1">"Click + to start one"</p>
                                        })}
                                    </div>
                                }.into_any()
                            }
                        } else {
                            view! {
                                <div>
                                    {session_list.into_iter().map(|s| {
                                        let sid = s.id.clone();
                                        let sid_for_check = sid.clone();
                                        let is_active = Signal::derive(move || {
                                            active_session_id.get().as_deref() == Some(sid_for_check.as_str())
                                        });
                                        view! {
                                            <SessionRow
                                                session=s
                                                is_active=is_active
                                                on_click=on_select
                                                renaming_id=renaming_id
                                                set_renaming_id=set_renaming_id
                                                confirming_delete_id=confirming_delete_id
                                                set_confirming_delete_id=set_confirming_delete_id
                                                set_sessions_version=set_sessions_version
                                                set_active_session_id=set_active_session_id
                                                active_session_id=active_session_id
                                            />
                                        }
                                    }).collect_view()}
                                </div>
                            }.into_any()
                        }
                    }}
                </Suspense>
            </div>
        </aside>
    }
}
