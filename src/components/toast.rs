use leptos::prelude::*;

/// Severity level for a toast notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Warning,
    Error,
}

impl ToastLevel {
    /// Returns the CSS class name from `style/main.css` for this level.
    fn css_class(&self) -> &'static str {
        match self {
            ToastLevel::Info => "toast-info",
            ToastLevel::Warning => "toast-warning",
            ToastLevel::Error => "toast-error",
        }
    }
}

/// A single toast notification.
#[derive(Clone, Debug)]
pub struct Toast {
    pub id: String,
    pub message: String,
    pub level: ToastLevel,
    pub ttl_ms: u64,
}

/// Manages the list of active toasts via context signals.
///
/// Context is provided as separate `ReadSignal<Vec<Toast>>` and
/// `WriteSignal<Vec<Toast>>` values, matching the existing codebase pattern.
pub struct ToastStore;

impl ToastStore {
    /// Creates the signal pair and provides both halves in context.
    fn provide() {
        let (read, write) = signal::<Vec<Toast>>(Vec::new());
        provide_context(read);
        provide_context(write);
    }

    /// Pushes a toast with an auto-generated ID and default TTL (4 000 ms).
    pub fn push(message: String, level: ToastLevel) {
        let write = use_context::<WriteSignal<Vec<Toast>>>().expect("toast WriteSignal context");
        let id = uuid::Uuid::new_v4().to_string();
        let toast = Toast {
            id,
            message,
            level,
            ttl_ms: 4000,
        };
        write.update(|list| list.push(toast));
    }

    /// Removes the toast with the given `id`, if present.
    pub fn dismiss(id: &str) {
        let write = use_context::<WriteSignal<Vec<Toast>>>().expect("toast WriteSignal context");
        let owned = id.to_string();
        write.update(|list| list.retain(|t| t.id != owned));
    }
}

/// Returns a closure that components can call to show a toast.
///
/// ```rust,ignore
/// let toast = use_toast();
/// toast("Something happened".into(), ToastLevel::Info);
/// ```
pub fn use_toast() -> impl Fn(String, ToastLevel) + Clone + 'static {
    let write = use_context::<WriteSignal<Vec<Toast>>>().expect("toast WriteSignal context");
    move |message: String, level: ToastLevel| {
        let id = uuid::Uuid::new_v4().to_string();
        let toast = Toast {
            id,
            message,
            level,
            ttl_ms: 4000,
        };
        write.update(|list| list.push(toast));
    }
}

/// Renders the stack of active toasts in a fixed overlay.
#[component]
fn ToastContainer() -> impl IntoView {
    let toasts = use_context::<ReadSignal<Vec<Toast>>>().expect("toast ReadSignal context");

    view! {
        <div class="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 flex flex-col-reverse items-center gap-2 pointer-events-none">
            <For
                each=move || toasts.get()
                key=|t| t.id.clone()
                children=move |toast: Toast| {
                    let id = toast.id.clone();
                    let dismiss_id = id.clone();
                    let auto_id = id.clone();
                    let css = format!(
                        "toast {} flex items-center gap-2 pointer-events-auto",
                        toast.level.css_class(),
                    );

                    // Auto-dismiss after TTL (client-side only)
                    let ttl = toast.ttl_ms;
                    cfg_if::cfg_if! {
                        if #[cfg(feature = "hydrate")] {
                            let write = use_context::<WriteSignal<Vec<Toast>>>()
                                .expect("toast WriteSignal context");
                            leptos::task::spawn_local(async move {
                                gloo_timers::future::TimeoutFuture::new(ttl as u32).await;
                                write.update(|list| list.retain(|t| t.id != auto_id));
                            });
                        } else {
                            let _ = (ttl, auto_id);
                        }
                    }

                    view! {
                        <div class=css>
                            <span>{toast.message.clone()}</span>
                            <button
                                class="ml-1 p-0.5 rounded hover:bg-white/10 transition-colors"
                                on:click=move |_| {
                                    ToastStore::dismiss(&dismiss_id);
                                }
                            >
                                <svg xmlns="http://www.w3.org/2000/svg" class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/>
                                </svg>
                            </button>
                        </div>
                    }
                }
            />
        </div>
    }
}

/// Wraps children with the toast context and renders the overlay container.
///
/// Place this near the root of your component tree:
/// ```rust,ignore
/// view! {
///     <ToastProvider>
///         <App />
///     </ToastProvider>
/// }
/// ```
#[component]
pub fn ToastProvider(children: Children) -> impl IntoView {
    ToastStore::provide();

    view! {
        {children()}
        <ToastContainer />
    }
}
